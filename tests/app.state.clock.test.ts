import { afterEach, describe, expect, it } from "bun:test";
import { mkdir, mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { persistProjection } from "../src/app/state";
import type { State } from "../src/types";

const repos: string[] = [];

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-clock-"));
  repos.push(repo);
  return repo;
}

afterEach(async () => {
  await Promise.all(repos.splice(0).map((r) => rm(r, { recursive: true, force: true })));
});

function emptyState(): State {
  return {
    tasks: {},
    deps: {},
    links: {},
    child_counters: {},
    created_order: [],
    applied_events: 0,
  };
}

describe("persistProjection clock injection", () => {
  it("uses injected clock for snapshot taken_at timestamp", async () => {
    const repo = await makeRepo();
    const tasqueDir = join(repo, ".tasque");
    await mkdir(tasqueDir, { recursive: true });
    await Bun.write(
      join(tasqueDir, "config.json"),
      JSON.stringify({ schema_version: 1, snapshot_every: 2 }),
    );

    const fixedDate = new Date("2026-01-15T12:00:00.000Z");
    const clock = () => fixedDate;

    await persistProjection(repo, emptyState(), 2, clock);

    const snapshotsDir = join(tasqueDir, "snapshots");
    const entries = await readdir(snapshotsDir);
    const snapFiles = entries.filter((name) => name.endsWith(".json"));
    expect(snapFiles.length).toBe(1);

    const content = JSON.parse(await readFile(join(snapshotsDir, snapFiles[0] as string), "utf8"));
    expect(content.taken_at).toBe("2026-01-15T12:00:00.000Z");
  });

  it("defaults to current time when no clock is provided", async () => {
    const repo = await makeRepo();
    const tasqueDir = join(repo, ".tasque");
    await mkdir(tasqueDir, { recursive: true });
    await Bun.write(
      join(tasqueDir, "config.json"),
      JSON.stringify({ schema_version: 1, snapshot_every: 1 }),
    );

    const before = new Date();
    await persistProjection(repo, emptyState(), 1);
    const after = new Date();

    const snapshotsDir = join(tasqueDir, "snapshots");
    const entries = await readdir(snapshotsDir);
    const snapFiles = entries.filter((name) => name.endsWith(".json"));
    expect(snapFiles.length).toBe(1);

    const content = JSON.parse(await readFile(join(snapshotsDir, snapFiles[0] as string), "utf8"));
    const takenAt = new Date(content.taken_at);
    expect(takenAt.getTime()).toBeGreaterThanOrEqual(before.getTime());
    expect(takenAt.getTime()).toBeLessThanOrEqual(after.getTime());
  });
});
