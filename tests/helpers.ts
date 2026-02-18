import { expect } from "bun:test";
import { existsSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

export interface JsonEnvelope {
  schema_version: number;
  command: string;
  ok: boolean;
  data?: unknown;
  error?: {
    code: string;
    message: string;
    details?: unknown;
  };
}

export interface JsonResult {
  exitCode: number;
  stdout: string;
  stderr: string;
  envelope: JsonEnvelope;
}

export interface CliResult {
  exitCode: number;
  stdout: string;
  stderr: string;
}

const repos: string[] = [];
const repoRoot = resolve(import.meta.dir, "..");
export const cliEntry = join(repoRoot, "src", "main.ts");

/**
 * Resolve the CLI command to use for testing.
 * Priority:
 * 1. TSQ_TEST_BIN environment variable (path to compiled binary)
 * 2. dist/tsq.exe if it exists (Windows compiled binary)
 * 3. Fallback to "bun run src/main.ts"
 */
function resolveCliCommand(): string[] {
  // Check environment variable first
  const testBin = process.env.TSQ_TEST_BIN;
  if (testBin) {
    return [testBin];
  }

  // Check for compiled binary at dist/tsq.exe
  const compiledBinary = join(repoRoot, "dist", "tsq.exe");
  if (existsSync(compiledBinary)) {
    return [compiledBinary];
  }

  // Fallback to bun run
  return ["bun", "run", cliEntry];
}

export const cliCmd = resolveCliCommand();

// Warm up compiled binary so the first test doesn't bear cold-start cost
// (OS process cache, AV scan on Windows, Bun spawn infrastructure).
if (cliCmd.length === 1) {
  Bun.spawnSync({ cmd: [...cliCmd, "--help"], stdout: "ignore", stderr: "ignore" });
}

export function makeRepo(prefix = "tasque-test-"): Promise<string> {
  return mkdtemp(join(tmpdir(), prefix)).then((repo) => {
    repos.push(repo);
    return repo;
  });
}

export async function cleanupRepos(): Promise<void> {
  await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
}

export function assertEnvelopeShape(value: unknown): asserts value is JsonEnvelope {
  expect(value).toBeObject();
  const envelope = value as Record<string, unknown>;
  expect(envelope.schema_version).toBe(1);
  expect(typeof envelope.command).toBe("string");
  expect(typeof envelope.ok).toBe("boolean");
  if (envelope.ok === true) {
    expect("data" in envelope).toBe(true);
  } else {
    expect("error" in envelope).toBe(true);
  }
}

export async function runJson(
  repoDir: string,
  args: string[],
  actor = "test",
  stdinText?: string,
): Promise<JsonResult> {
  // Compiled Bun executables can drop piped stdin under bun test on Windows.
  // Keep compiled coverage for normal paths, but force source-run for stdin cases.
  const cmd = stdinText === undefined ? cliCmd : ["bun", "run", cliEntry];
  const proc = Bun.spawn({
    cmd: [...cmd, ...args, "--json"],
    cwd: repoDir,
    env: {
      ...process.env,
      TSQ_ACTOR: actor,
    },
    stdin: stdinText === undefined ? "ignore" : new Blob([stdinText]),
    stdout: "pipe",
    stderr: "pipe",
  });

  const [exitCode, stdout, stderr] = await Promise.all([
    proc.exited,
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
  ]);

  const trimmed = stdout.trim();
  expect(trimmed.length > 0).toBe(true);
  const parsed = JSON.parse(trimmed) as unknown;
  assertEnvelopeShape(parsed);

  return {
    exitCode,
    stdout,
    stderr,
    envelope: parsed,
  };
}

export async function runCli(repoDir: string, args: string[], actor = "test"): Promise<CliResult> {
  const proc = Bun.spawn({
    cmd: [...cliCmd, ...args],
    cwd: repoDir,
    env: {
      ...process.env,
      TSQ_ACTOR: actor,
    },
    stdout: "pipe",
    stderr: "pipe",
  });

  const [exitCode, stdout, stderr] = await Promise.all([
    proc.exited,
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
  ]);

  return {
    exitCode,
    stdout,
    stderr,
  };
}

export function okData<T>(envelope: JsonEnvelope): T {
  expect(envelope.ok).toBe(true);
  return envelope.data as T;
}
