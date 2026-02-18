import { mkdir, open, readFile, rename, unlink } from "node:fs/promises";

import { TsqError } from "../errors";
import { type Config, SCHEMA_VERSION } from "../types";
import { getPaths } from "./paths";

const DEFAULT_CONFIG: Config = {
  schema_version: SCHEMA_VERSION,
  snapshot_every: 200,
};

function isConfig(value: unknown): value is Config {
  if (!value || typeof value !== "object") {
    return false;
  }

  const config = value as Partial<Config>;
  return (
    typeof config.schema_version === "number" &&
    typeof config.snapshot_every === "number" &&
    Number.isInteger(config.snapshot_every) &&
    config.snapshot_every > 0
  );
}

export async function writeDefaultConfig(repoRoot: string): Promise<void> {
  const paths = getPaths(repoRoot);
  await mkdir(paths.tasqueDir, { recursive: true });

  try {
    await readFile(paths.configFile, "utf8");
    return;
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code !== "ENOENT") {
      throw new TsqError("CONFIG_READ_FAILED", "Failed checking config", 2, error);
    }
  }

  const temp = `${paths.configFile}.tmp-${process.pid}-${Date.now()}`;
  const payload = `${JSON.stringify(DEFAULT_CONFIG, null, 2)}\n`;

  try {
    const handle = await open(temp, "w");
    try {
      await handle.writeFile(payload, "utf8");
      await handle.sync();
    } finally {
      await handle.close();
    }
    await rename(temp, paths.configFile);
  } catch (error) {
    try {
      await unlink(temp);
    } catch {
      // best-effort cleanup
    }
    throw new TsqError("CONFIG_WRITE_FAILED", "Failed writing default config", 2, error);
  }
}

export async function readConfig(repoRoot: string): Promise<Config> {
  const paths = getPaths(repoRoot);

  let raw: string;
  try {
    raw = await readFile(paths.configFile, "utf8");
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "ENOENT") {
      await writeDefaultConfig(repoRoot);
      return { ...DEFAULT_CONFIG };
    }
    throw new TsqError("CONFIG_READ_FAILED", "Failed reading config", 2, error);
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    throw new TsqError("CONFIG_INVALID", "Config JSON is malformed", 2, error);
  }

  if (!isConfig(parsed)) {
    throw new TsqError("CONFIG_INVALID", "Config shape is invalid", 2, parsed);
  }

  return parsed;
}
