import { mkdir, open, readFile, unlink } from "node:fs/promises";
import { hostname } from "node:os";

import { TsqError } from "../errors";
import { getPaths } from "./paths";

const LOCK_TIMEOUT_MS = 3000;
const STALE_LOCK_MS = 30000;
const JITTER_MIN_MS = 20;
const JITTER_MAX_MS = 80;

interface LockPayload {
  host: string;
  pid: number;
  created_at: string;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function jitterMs(): number {
  return Math.floor(Math.random() * (JITTER_MAX_MS - JITTER_MIN_MS + 1)) + JITTER_MIN_MS;
}

function parseLockPayload(raw: string): LockPayload | null {
  try {
    const parsed = JSON.parse(raw) as Partial<LockPayload>;
    if (
      typeof parsed.host !== "string" ||
      typeof parsed.pid !== "number" ||
      typeof parsed.created_at !== "string"
    ) {
      return null;
    }
    return {
      host: parsed.host,
      pid: parsed.pid,
      created_at: parsed.created_at,
    };
  } catch {
    return null;
  }
}

function isProcessDead(pid: number): boolean {
  if (pid === process.pid) {
    return false;
  }

  try {
    process.kill(pid, 0);
    return false;
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "ESRCH") {
      return true;
    }
    if (code === "EPERM") {
      return false;
    }
    return false;
  }
}

async function tryCleanupStaleLock(lockFile: string, currentHost: string): Promise<boolean> {
  let raw: string;
  try {
    raw = await readFile(lockFile, "utf8");
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "ENOENT") {
      return true;
    }
    return false;
  }

  const payload = parseLockPayload(raw);
  if (!payload || payload.host !== currentHost) {
    return false;
  }

  const createdAtMs = Date.parse(payload.created_at);
  if (!Number.isFinite(createdAtMs)) {
    return false;
  }

  if (Date.now() - createdAtMs < STALE_LOCK_MS) {
    return false;
  }

  if (!isProcessDead(payload.pid)) {
    return false;
  }

  try {
    await unlink(lockFile);
    return true;
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "ENOENT") {
      return true;
    }
    return false;
  }
}

async function acquireWriteLock(lockFile: string, tasqueDir: string): Promise<LockPayload> {
  const deadline = Date.now() + LOCK_TIMEOUT_MS;
  const host = hostname();

  while (true) {
    const payload: LockPayload = {
      host,
      pid: process.pid,
      created_at: new Date().toISOString(),
    };

    try {
      await mkdir(tasqueDir, { recursive: true });
      const handle = await open(lockFile, "wx");
      try {
        await handle.writeFile(`${JSON.stringify(payload)}\n`, "utf8");
        await handle.sync();
      } finally {
        await handle.close();
      }
      return payload;
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (code !== "EEXIST") {
        throw new TsqError("LOCK_ACQUIRE_FAILED", "Failed to acquire write lock", 2, error);
      }

      const cleaned = await tryCleanupStaleLock(lockFile, host);
      if (cleaned) {
        continue;
      }

      if (Date.now() >= deadline) {
        throw new TsqError("LOCK_TIMEOUT", "Timed out acquiring write lock", 2, {
          lockFile,
          timeout_ms: LOCK_TIMEOUT_MS,
        });
      }

      await sleep(jitterMs());
    }
  }
}

async function releaseWriteLock(lockFile: string, ownedLock: LockPayload): Promise<void> {
  let raw: string;
  try {
    raw = await readFile(lockFile, "utf8");
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "ENOENT") {
      return;
    }
    throw new TsqError("LOCK_RELEASE_FAILED", "Failed reading lock file on release", 2, error);
  }

  const payload = parseLockPayload(raw);
  if (
    !payload ||
    payload.host !== ownedLock.host ||
    payload.pid !== ownedLock.pid ||
    payload.created_at !== ownedLock.created_at
  ) {
    return;
  }

  try {
    await unlink(lockFile);
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "ENOENT") {
      return;
    }
    throw new TsqError("LOCK_RELEASE_FAILED", "Failed removing lock file", 2, error);
  }
}

export async function forceRemoveLock(
  repoRoot: string,
): Promise<{ host: string; pid: number; created_at: string } | null> {
  const paths = getPaths(repoRoot);
  let raw: string;
  try {
    raw = await readFile(paths.lockFile, "utf8");
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "ENOENT") {
      return null;
    }
    throw new TsqError("LOCK_REMOVE_FAILED", "Failed reading lock file", 2, error);
  }

  const payload = parseLockPayload(raw);
  try {
    await unlink(paths.lockFile);
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code !== "ENOENT") {
      throw new TsqError("LOCK_REMOVE_FAILED", "Failed removing lock file", 2, error);
    }
  }
  return payload;
}

export async function lockExists(repoRoot: string): Promise<boolean> {
  const paths = getPaths(repoRoot);
  try {
    await readFile(paths.lockFile, "utf8");
    return true;
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "ENOENT") {
      return false;
    }
    throw new TsqError("LOCK_CHECK_FAILED", `Failed checking lock file: ${code}`, 2, error);
  }
}

export async function withWriteLock<T>(repoRoot: string, fn: () => Promise<T>): Promise<T> {
  const paths = getPaths(repoRoot);
  const lock = await acquireWriteLock(paths.lockFile, paths.tasqueDir);
  try {
    return await fn();
  } finally {
    await releaseWriteLock(paths.lockFile, lock);
  }
}
