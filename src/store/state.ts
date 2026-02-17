import { mkdir, open, readFile, rename } from "node:fs/promises";

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
    throw new TsqError("STATE_WRITE_FAILED", "Failed writing state cache", 2, error);
  }
}

export async function readStateCache(repoRoot: string): Promise<State | null> {
  const paths = getPaths(repoRoot);
  try {
    const raw = await readFile(paths.stateFile, "utf8");
    try {
      return JSON.parse(raw) as State;
    } catch {
      // Corrupt cache is silently discarded; it will be rebuilt from events
      return null;
    }
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "ENOENT") {
      return null;
    }
    throw new TsqError("STATE_READ_FAILED", "Failed reading state cache", 2, error);
  }
}
