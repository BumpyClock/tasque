import { afterEach, describe, expect, it } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { loadLatestSnapshot, writeSnapshot } from "../src/store/snapshots";
import type { Snapshot, State } from "../src/types";

const repos: string[] = [];

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-store-snapshots-"));
  repos.push(repo);
  return repo;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

afterEach(async () => {
  await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
});

function state(appliedEvents: number): State {
  return {
    tasks: {},
    deps: {},
    links: {},
    child_counters: {},
    created_order: [],
    applied_events: appliedEvents,
  };
}

function snapshot(takenAt: string, eventCount: number, appliedEvents: number): Snapshot {
  return {
    taken_at: takenAt,
    event_count: eventCount,
    state: state(appliedEvents),
  };
}

describe("store snapshots", () => {
  it("writes and loads latest snapshot", async () => {
    const repo = await makeRepo();
    const first = snapshot("2026-02-17T00:00:00.000Z", 1, 1);
    const second = snapshot("2026-02-17T00:00:01.000Z", 2, 2);

    await writeSnapshot(repo, first);
    await sleep(20);
    await writeSnapshot(repo, second);

    const loaded = await loadLatestSnapshot(repo);
    expect(loaded).toEqual(second);
  });
});
