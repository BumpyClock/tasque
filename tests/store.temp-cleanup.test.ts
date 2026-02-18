import { afterEach, describe, expect, it } from "bun:test";
import { mkdir, mkdtemp, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { writeTaskSpecAtomic } from "../src/app/storage";
import { writeDefaultConfig } from "../src/store/config";
import { writeSnapshot } from "../src/store/snapshots";
import { writeStateCache } from "../src/store/state";
import type { Snapshot, State } from "../src/types";

const repos: string[] = [];

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-temp-cleanup-"));
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

async function listTmpFiles(dir: string): Promise<string[]> {
  try {
    const entries = await readdir(dir);
    return entries.filter((name) => name.includes(".tmp"));
  } catch {
    return [];
  }
}

describe("temp file cleanup on write failure", () => {
  it("writeStateCache leaves no temp files when rename target dir is missing", async () => {
    const repo = await makeRepo();
    const tasqueDir = join(repo, ".tasque");
    await mkdir(tasqueDir, { recursive: true });

    // Make the target path a directory so rename fails
    const stateFilePath = join(tasqueDir, "tasks.jsonl");
    await mkdir(stateFilePath, { recursive: true });

    let threw = false;
    try {
      await writeStateCache(repo, emptyState());
    } catch {
      threw = true;
    }
    expect(threw).toBe(true);

    const tmpFiles = await listTmpFiles(tasqueDir);
    expect(tmpFiles).toHaveLength(0);
  });

  it("writeSnapshot leaves no temp files when rename fails", async () => {
    const repo = await makeRepo();
    const snapshotsDir = join(repo, ".tasque", "snapshots");
    await mkdir(snapshotsDir, { recursive: true });

    const snap: Snapshot = {
      taken_at: "2026-02-18T00:00:00.000Z",
      event_count: 1,
      state: emptyState(),
    };

    // Create a directory at the target path to cause rename to fail
    const targetName = "2026-02-18T00-00-00-000Z-1.json";
    await mkdir(join(snapshotsDir, targetName), { recursive: true });

    let threw = false;
    try {
      await writeSnapshot(repo, snap);
    } catch {
      threw = true;
    }
    expect(threw).toBe(true);

    const tmpFiles = await listTmpFiles(snapshotsDir);
    expect(tmpFiles).toHaveLength(0);
  });

  it("writeDefaultConfig leaves no temp files when rename fails", async () => {
    const repo = await makeRepo();
    const tasqueDir = join(repo, ".tasque");
    await mkdir(tasqueDir, { recursive: true });

    // Create a directory at config.json path to cause rename to fail
    const configPath = join(tasqueDir, "config.json");
    await mkdir(configPath, { recursive: true });

    let threw = false;
    try {
      await writeDefaultConfig(repo);
    } catch {
      threw = true;
    }
    expect(threw).toBe(true);

    const tmpFiles = await listTmpFiles(tasqueDir);
    expect(tmpFiles).toHaveLength(0);
  });

  it("writeTaskSpecAtomic leaves no temp files when rename fails", async () => {
    const repo = await makeRepo();
    const specDir = join(repo, ".tasque", "specs", "tsq-test01");
    await mkdir(specDir, { recursive: true });

    // Create a directory at spec.md path to cause rename to fail
    const specFile = join(specDir, "spec.md");
    await mkdir(specFile, { recursive: true });

    let threw = false;
    try {
      await writeTaskSpecAtomic(repo, "tsq-test01", "# Overview\nTest spec");
    } catch {
      threw = true;
    }
    expect(threw).toBe(true);

    const tmpFiles = await listTmpFiles(specDir);
    expect(tmpFiles).toHaveLength(0);
  });
});
