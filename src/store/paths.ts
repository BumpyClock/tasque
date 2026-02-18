import { join } from "node:path";

export interface TasquePaths {
  tasqueDir: string;
  eventsFile: string;
  configFile: string;
  stateFile: string;
  lockFile: string;
  snapshotsDir: string;
  specsDir: string;
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
    specsDir: join(tasqueDir, "specs"),
  };
}

export function taskSpecRelativePath(taskId: string): string {
  return `.tasque/specs/${taskId}/spec.md`;
}

export function taskSpecFile(repoRoot: string, taskId: string): string {
  return join(repoRoot, ".tasque", "specs", taskId, "spec.md");
}
