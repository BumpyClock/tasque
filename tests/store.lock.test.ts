import { afterEach, describe, expect, it } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { withWriteLock } from "../src/store/lock";

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
});
