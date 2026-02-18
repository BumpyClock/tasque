import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { TsqError } from "../errors";

export const DEFAULT_SNAPSHOT_EVERY = 100;

export function nowIso(): string {
  return new Date().toISOString();
}

/** Walk up from cwd to find the nearest directory containing `.tasque/`. */
export function findTasqueRoot(): string | null {
  let dir = process.cwd();
  for (;;) {
    if (existsSync(join(dir, ".tasque"))) {
      return dir;
    }
    const parent = dirname(dir);
    if (parent === dir) {
      return null;
    }
    dir = parent;
  }
}

/**
 * Return the project root containing `.tasque/`.
 * Falls back to cwd when no `.tasque/` is found (needed for `tsq init`).
 */
export function getRepoRoot(): string {
  return findTasqueRoot() ?? process.cwd();
}

export function getActor(repoRoot: string): string {
  const fromEnv = process.env.TSQ_ACTOR?.trim();
  if (fromEnv) {
    return fromEnv;
  }

  const gitName = readGitUserName(repoRoot);
  if (gitName) {
    return gitName;
  }

  const osUser = process.env.USERNAME ?? process.env.USER;
  if (osUser && osUser.trim().length > 0) {
    return osUser.trim();
  }

  return "unknown";
}

export function parsePriority(raw: string | number): 0 | 1 | 2 | 3 {
  const value = typeof raw === "number" ? raw : Number.parseInt(raw, 10);
  if (value === 0 || value === 1 || value === 2 || value === 3) {
    return value;
  }
  throw new TsqError("VALIDATION_ERROR", "priority must be one of: 0, 1, 2, 3", 1);
}

export function normalizeStatus(raw: string) {
  const normalized = raw === "done" ? "closed" : raw === "todo" ? "open" : raw;
  if (
    normalized === "open" ||
    normalized === "in_progress" ||
    normalized === "blocked" ||
    normalized === "closed" ||
    normalized === "canceled" ||
    normalized === "deferred"
  ) {
    return normalized;
  }
  throw new TsqError(
    "VALIDATION_ERROR",
    "status must be one of: open, todo, in_progress, blocked, closed, done, canceled, deferred",
    1,
  );
}

function readGitUserName(repoRoot: string): string | null {
  try {
    const proc = Bun.spawnSync({
      cmd: ["git", "config", "user.name"],
      cwd: repoRoot,
      stdout: "pipe",
      stderr: "ignore",
    });
    if (proc.exitCode !== 0) {
      return null;
    }
    const value = Buffer.from(proc.stdout).toString("utf8").trim();
    return value.length > 0 ? value : null;
  } catch {
    return null;
  }
}
