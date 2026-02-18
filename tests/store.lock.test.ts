import { afterEach, describe, expect, it } from "bun:test";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
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
    const innerPromise = new Promise<void>((r) => { resolveInner = r; });

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
    const repo = await makeRepo();

    const holder = withWriteLock(repo, async () => {
      await sleep(3600);
    });

    await sleep(25);

    await expect(withWriteLock(repo, async () => {})).rejects.toMatchObject({
      code: "LOCK_TIMEOUT",
      exitCode: 3,
    });

    await holder;
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
  });
});
