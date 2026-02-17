import { join } from "node:path";

export interface TasquePaths {
  tasqueDir: string;
  eventsFile: string;
  configFile: string;
  stateFile: string;
  lockFile: string;
  snapshotsDir: string;
}

export function getPaths(repoRoot: string): TasquePaths {
  const tasqueDir = join(repoRoot, ".tasque");
  return {
    tasqueDir,
    eventsFile: join(tasqueDir, "events.jsonl"),
    configFile: join(tasqueDir, "config.json"),
    stateFile: join(tasqueDir, "state.json"),
    lockFile: join(tasqueDir, ".lock"),
    snapshotsDir: join(tasqueDir, "snapshots"),
  };
}
