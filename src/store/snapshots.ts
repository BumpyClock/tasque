import { mkdir, open, readFile, readdir, rename, stat } from "node:fs/promises";
import { join } from "node:path";

import { TsqError } from "../errors";
import type { Snapshot } from "../types";
import { getPaths } from "./paths";

function snapshotFilename(snapshot: Snapshot): string {
  const ts = snapshot.taken_at.replace(/[:.]/g, "-");
  return `${ts}-${snapshot.event_count}.json`;
}

export async function loadLatestSnapshot(repoRoot: string): Promise<Snapshot | null> {
  const paths = getPaths(repoRoot);
  let entries: string[];

  try {
    entries = await readdir(paths.snapshotsDir);
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "ENOENT") {
      return null;
    }
    throw new TsqError("SNAPSHOT_READ_FAILED", "Failed listing snapshots", 2, error);
  }

  const candidates = entries.filter((name) => name.endsWith(".json"));
  if (candidates.length === 0) {
    return null;
  }

  const withMtime = await Promise.all(
    candidates.map(async (name) => {
      const file = join(paths.snapshotsDir, name);
      const details = await stat(file);
      return { file, mtimeMs: details.mtimeMs };
    }),
  );

  withMtime.sort((a, b) => b.mtimeMs - a.mtimeMs);
  const latest = withMtime[0]?.file;
  if (!latest) {
    return null;
  }

  try {
    const raw = await readFile(latest, "utf8");
    return JSON.parse(raw) as Snapshot;
  } catch (error) {
    throw new TsqError("SNAPSHOT_READ_FAILED", "Failed reading latest snapshot", 2, error);
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
  } catch (error) {
    throw new TsqError("SNAPSHOT_WRITE_FAILED", "Failed writing snapshot", 2, error);
  }
}
