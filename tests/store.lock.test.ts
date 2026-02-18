import { afterEach, describe, expect, it } from "bun:test";
import { mkdir, mkdtemp, rm, unlink, writeFile } from "node:fs/promises";
import { hostname, tmpdir } from "node:os";
import { join } from "node:path";

import { lockExists, withWriteLock } from "../src/store/lock";
import { getPaths } from "../src/store/paths";

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

const repos: string[] = [];

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-store-lock-"));
  repos.push(repo);
  return repo;
}

afterEach(async () => {
  await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
});

describe("store lock", () => {
  it("serializes concurrent writers", async () => {
    const repo = await makeRepo();
    const order: string[] = [];

    const holder = withWriteLock(repo, async () => {
      order.push("holder-start");
      await sleep(250);
      order.push("holder-end");
    });

    await sleep(25);

    const waiter = withWriteLock(repo, async () => {
      order.push("waiter");
    });

    await Promise.all([holder, waiter]);
    expect(order).toEqual(["holder-start", "holder-end", "waiter"]);
  });

  it("lockExists returns false when no lock file exists", async () => {
    const repo = await makeRepo();
    const result = await lockExists(repo);
    expect(result).toBe(false);
  });

  it("lockExists returns true when lock is held", async () => {
    const repo = await makeRepo();
    let resolveInner: (() => void) | undefined;
    const innerPromise = new Promise<void>((r) => {
      resolveInner = r;
    });

    const holder = withWriteLock(repo, async () => {
      await innerPromise;
    });

    await sleep(50);
    const result = await lockExists(repo);
    expect(result).toBe(true);

    resolveInner!();
    await holder;
  });

  it("times out when lock is held too long", async () => {
    process.env.TSQ_LOCK_TIMEOUT_MS = "500";
    try {
      const repo = await makeRepo();

      const holder = withWriteLock(repo, async () => {
        await sleep(1000);
      });

      await sleep(25);

      await expect(withWriteLock(repo, async () => {})).rejects.toMatchObject({
        code: "LOCK_TIMEOUT",
        exitCode: 3,
      });

      await holder;
    } finally {
      process.env.TSQ_LOCK_TIMEOUT_MS = undefined;
    }
  });

  it("cleans up stale lock from dead PID on same host and allows new lock acquisition", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });

    const staleLockPayload = {
      host: hostname(),
      pid: 4294967,
      created_at: new Date(Date.now() - 60_000).toISOString(),
    };
    await writeFile(paths.lockFile, `${JSON.stringify(staleLockPayload)}\n`, "utf8");

    let acquired = false;
    await withWriteLock(repo, async () => {
      acquired = true;
    });

    expect(acquired).toBe(true);
    expect(await lockExists(repo)).toBe(false);
  });

  it("does not clean up lock held by current PID and times out instead", async () => {
    process.env.TSQ_LOCK_TIMEOUT_MS = "500";
    try {
      const repo = await makeRepo();
      const paths = getPaths(repo);
      await mkdir(paths.tasqueDir, { recursive: true });

      const currentPidLockPayload = {
        host: hostname(),
        pid: process.pid,
        created_at: new Date(Date.now() - 60_000).toISOString(),
      };
      await writeFile(paths.lockFile, `${JSON.stringify(currentPidLockPayload)}\n`, "utf8");

      await expect(withWriteLock(repo, async () => {})).rejects.toMatchObject({
        code: "LOCK_TIMEOUT",
        exitCode: 3,
      });

      expect(await lockExists(repo)).toBe(true);
    } finally {
      process.env.TSQ_LOCK_TIMEOUT_MS = undefined;
    }
  });

  it("throws callback error when only callback fails", async () => {
    const repo = await makeRepo();
    const callbackErr = new Error("callback-boom");

    await expect(
      withWriteLock(repo, async () => {
        throw callbackErr;
      }),
    ).rejects.toBe(callbackErr);

    expect(await lockExists(repo)).toBe(false);
  });

  it("throws release error when only release fails", async () => {
    const repo = await makeRepo();

    await expect(
      withWriteLock(repo, async () => {
        const paths = getPaths(repo);
        // Replace lock file with a directory so readFile fails with EISDIR
        await unlink(paths.lockFile);
        await mkdir(paths.lockFile);
      }),
    ).rejects.toMatchObject({
      code: "LOCK_RELEASE_FAILED",
    });
  });

  it("throws AggregateError when both callback and release fail", async () => {
    const repo = await makeRepo();
    const callbackErr = new Error("callback-boom");

    try {
      await withWriteLock(repo, async () => {
        const paths = getPaths(repo);
        // Replace lock file with a directory so release also fails
        await unlink(paths.lockFile);
        await mkdir(paths.lockFile);
        throw callbackErr;
      });
      expect.unreachable("should have thrown");
    } catch (err) {
      expect(err).toBeInstanceOf(AggregateError);
      const agg = err as AggregateError;
      expect(agg.errors).toHaveLength(2);
      expect(agg.errors[0]).toBe(callbackErr);
      expect((agg.errors[1] as { code: string }).code).toBe("LOCK_RELEASE_FAILED");
    }
  });

  it("concurrent stale-lock cleanup yields exactly one lock owner at a time", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });

    // Plant a stale lock from a dead PID
    const staleLockPayload = {
      host: hostname(),
      pid: 4294967,
      created_at: new Date(Date.now() - 60_000).toISOString(),
    };
    await writeFile(paths.lockFile, `${JSON.stringify(staleLockPayload)}\n`, "utf8");

    let concurrentHolders = 0;
    let maxConcurrentHolders = 0;

    // Launch multiple concurrent withWriteLock calls that all see the stale lock
    const tasks = Array.from({ length: 5 }, (_, i) =>
      withWriteLock(repo, async () => {
        concurrentHolders++;
        maxConcurrentHolders = Math.max(maxConcurrentHolders, concurrentHolders);
        // Hold the lock briefly so contention is possible
        await sleep(50);
        concurrentHolders--;
        return i;
      }),
    );

    const results = await Promise.all(tasks);

    // All 5 should complete successfully
    expect(results.sort()).toEqual([0, 1, 2, 3, 4]);
    // At most one holder at a time (single-writer guarantee)
    expect(maxConcurrentHolders).toBe(1);
  });
});
