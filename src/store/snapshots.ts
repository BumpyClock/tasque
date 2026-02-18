import { mkdir, open, readFile, readdir, rename, unlink } from "node:fs/promises";
import { join } from "node:path";

import { TsqError } from "../errors";
import type { Snapshot } from "../types";
import { getPaths } from "./paths";

export const SNAPSHOT_RETAIN_COUNT = 5;

export interface LoadedSnapshot {
  snapshot: Snapshot | null;
  warning?: string;
}

function snapshotFilename(snapshot: Snapshot): string {
  const ts = snapshot.taken_at.replace(/[:.]/g, "-");
  return `${ts}-${snapshot.event_count}.json`;
}

export async function loadLatestSnapshot(repoRoot: string): Promise<Snapshot | null> {
  const result = await loadLatestSnapshotWithWarning(repoRoot);
  return result.snapshot;
}

export async function loadLatestSnapshotWithWarning(repoRoot: string): Promise<LoadedSnapshot> {
  const paths = getPaths(repoRoot);
  let entries: string[];

  try {
    entries = await readdir(paths.snapshotsDir);
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "ENOENT") {
      return { snapshot: null };
    }
    throw new TsqError("SNAPSHOT_READ_FAILED", "Failed listing snapshots", 2, error);
  }

  const candidates = entries.filter((name) => name.endsWith(".json"));
  if (candidates.length === 0) {
    return { snapshot: null };
  }

  // Filenames encode ISO timestamps; lexicographic sort gives chronological order
  candidates.sort();
  const invalid: string[] = [];
  for (let idx = candidates.length - 1; idx >= 0; idx -= 1) {
    const name = candidates[idx];
    if (!name) {
      continue;
    }
    const candidate = join(paths.snapshotsDir, name);
    try {
      const raw = await readFile(candidate, "utf8");
      const parsed = JSON.parse(raw);
      if (!isSnapshot(parsed)) {
        invalid.push(name);
        continue;
      }
      return {
        snapshot: parsed,
        warning: invalid.length > 0 ? invalidSnapshotWarning(invalid) : undefined,
      };
    } catch {
      invalid.push(name);
    }
  }

  return {
    snapshot: null,
    warning: invalid.length > 0 ? invalidSnapshotWarning(invalid) : undefined,
  };
}

function isSnapshot(value: unknown): value is Snapshot {
  if (!value || typeof value !== "object") {
    return false;
  }
  const snapshot = value as Partial<Snapshot>;
  if (typeof snapshot.taken_at !== "string") {
    return false;
  }
  if (typeof snapshot.event_count !== "number") {
    return false;
  }
  if (!snapshot.state || typeof snapshot.state !== "object") {
    return false;
  }
  return true;
}

function invalidSnapshotWarning(invalidNames: string[]): string {
  const first = invalidNames.slice(0, 3).join(",");
  const overflow = invalidNames.length - 3;
  if (overflow > 0) {
    return `Ignored invalid snapshot files: ${first} (+${overflow} more)`;
  }
  return `Ignored invalid snapshot files: ${first}`;
}

async function pruneSnapshots(paths: ReturnType<typeof getPaths>): Promise<void> {
  let entries: string[];
  try {
    entries = await readdir(paths.snapshotsDir);
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "ENOENT") {
      return;
    }
    console.warn(`Warning: failed to prune snapshots (${code}): ${(error as Error).message}`);
    return;
  }

  const snapshots = entries.filter((name) => name.endsWith(".json")).sort();
  if (snapshots.length <= SNAPSHOT_RETAIN_COUNT) {
    return;
  }

  const stale = snapshots.slice(0, snapshots.length - SNAPSHOT_RETAIN_COUNT);
  for (const name of stale) {
    try {
      await unlink(join(paths.snapshotsDir, name));
    } catch {
      // best-effort prune; repair remains the backstop
    }
  }
}

export async function writeSnapshot(repoRoot: string, snapshot: Snapshot): Promise<void> {
  const paths = getPaths(repoRoot);
  await mkdir(paths.snapshotsDir, { recursive: true });

  const target = join(paths.snapshotsDir, snapshotFilename(snapshot));
  const temp = `${target}.tmp-${process.pid}-${Date.now()}`;
  const payload = `${JSON.stringify(snapshot, null, 2)}\n`;

  try {
    const handle = await open(temp, "w");
    try {
      await handle.writeFile(payload, "utf8");
      await handle.sync();
    } finally {
      await handle.close();
    }
    await rename(temp, target);
    await pruneSnapshots(paths);
  } catch (error) {
    try {
      await unlink(temp);
    } catch {
      // best-effort cleanup
    }
    throw new TsqError("SNAPSHOT_WRITE_FAILED", "Failed writing snapshot", 2, error);
  }
}
