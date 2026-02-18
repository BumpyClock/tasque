import { mkdir, open, readFile, rename, unlink } from "node:fs/promises";
import { join } from "node:path";

import { TsqError } from "../errors";
import type { State } from "../types";
import { getPaths } from "./paths";

export async function writeStateCache(repoRoot: string, state: State): Promise<void> {
  const paths = getPaths(repoRoot);
  await mkdir(paths.tasqueDir, { recursive: true });

  const temp = `${paths.stateFile}.tmp-${process.pid}-${Date.now()}`;
  const payload = `${JSON.stringify(state, null, 2)}\n`;

  try {
    const handle = await open(temp, "w");
    try {
      await handle.writeFile(payload, "utf8");
      await handle.sync();
    } finally {
      await handle.close();
    }
    await rename(temp, paths.stateFile);
  } catch (error) {
    try {
      await unlink(temp);
    } catch {
      // best-effort cleanup
    }
    throw new TsqError("STATE_WRITE_FAILED", "Failed writing state cache", 2, error);
  }
}

export async function readStateCache(repoRoot: string): Promise<State | null> {
  const paths = getPaths(repoRoot);
  const legacyStateFile = join(paths.tasqueDir, "tasks.jsonl");
  const candidates = [paths.stateFile, legacyStateFile];
  for (const stateFile of candidates) {
    try {
      const raw = await readFile(stateFile, "utf8");
      try {
        return JSON.parse(raw) as State;
      } catch {
        // Corrupt cache is silently discarded; it will be rebuilt from events
        return null;
      }
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (code === "ENOENT") {
        continue;
      }
      throw new TsqError("STATE_READ_FAILED", "Failed reading state cache", 2, error);
    }
  }
  return null;
}
