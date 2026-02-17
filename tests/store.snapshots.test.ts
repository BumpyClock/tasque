import { afterEach, describe, expect, it } from "bun:test";
import { mkdir, mkdtemp, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  SNAPSHOT_RETAIN_COUNT,
  loadLatestSnapshot,
  loadLatestSnapshotWithWarning,
  writeSnapshot,
} from "../src/store/snapshots";
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

function snapshotFilename(takenAt: string, eventCount: number): string {
  return `${takenAt.replace(/[:.]/g, "-")}-${eventCount}.json`;
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

  it("picks latest snapshot by filename sort regardless of write order", async () => {
    const repo = await makeRepo();
    // Write later-timestamped snapshot first, earlier-timestamped second
    const later = snapshot("2026-02-17T12:00:00.000Z", 200, 200);
    const earlier = snapshot("2026-02-17T06:00:00.000Z", 100, 100);

    await writeSnapshot(repo, later);
    await sleep(20);
    await writeSnapshot(repo, earlier);

    const loaded = await loadLatestSnapshot(repo);
    // Should pick "later" by filename even though "earlier" has newer mtime
    expect(loaded).toEqual(later);
  });

  it("falls back to older valid snapshot when newest snapshot file is malformed", async () => {
    const repo = await makeRepo();
    const snapshotsDir = join(repo, ".tasque", "snapshots");
    const older = snapshot("2026-02-17T12:00:00.000Z", 200, 200);
    const newerName = snapshotFilename("2026-02-17T12:00:01.000Z", 201);

    await writeSnapshot(repo, older);
    await mkdir(snapshotsDir, { recursive: true });
    await writeFile(join(snapshotsDir, newerName), "{not-json", "utf8");

    const loaded = await loadLatestSnapshot(repo);
    expect(loaded).toEqual(older);

    const withWarning = await loadLatestSnapshotWithWarning(repo);
    expect(withWarning.snapshot).toEqual(older);
    expect(withWarning.warning).toContain(newerName);
  });

  it("returns null snapshot with warning when all snapshot files are invalid", async () => {
    const repo = await makeRepo();
    const snapshotsDir = join(repo, ".tasque", "snapshots");
    const newest = snapshotFilename("2026-02-17T12:00:02.000Z", 202);
    const older = snapshotFilename("2026-02-17T12:00:01.000Z", 201);

    await mkdir(snapshotsDir, { recursive: true });
    await writeFile(join(snapshotsDir, newest), "{broken-json", "utf8");
    await writeFile(join(snapshotsDir, older), JSON.stringify({ foo: "bar" }), "utf8");

    const loaded = await loadLatestSnapshot(repo);
    expect(loaded).toBeNull();

    const withWarning = await loadLatestSnapshotWithWarning(repo);
    expect(withWarning.snapshot).toBeNull();
    expect(withWarning.warning).toContain("Ignored invalid snapshot files");
    expect(withWarning.warning).toContain(newest);
    expect(withWarning.warning).toContain(older);
  });

  it("keeps only the latest five snapshot json files on snapshot write", async () => {
    const repo = await makeRepo();
    const snapshotsDir = join(repo, ".tasque", "snapshots");

    for (let eventCount = 1; eventCount <= 7; eventCount += 1) {
      const takenAt = `2026-02-17T00:00:0${eventCount}.000Z`;
      await writeSnapshot(repo, snapshot(takenAt, eventCount, eventCount));
    }

    const files = (await readdir(snapshotsDir)).filter((name) => name.endsWith(".json")).sort();
    const expected = [3, 4, 5, 6, 7].map((eventCount) =>
      snapshotFilename(`2026-02-17T00:00:0${eventCount}.000Z`, eventCount),
    );

    expect(files).toHaveLength(SNAPSHOT_RETAIN_COUNT);
    expect(files).toEqual(expected);
  });
});
