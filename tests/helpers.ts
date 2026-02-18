import { expect } from "bun:test";
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
  const proc = Bun.spawn({
    cmd: ["bun", "run", cliEntry, ...args, "--json"],
    cwd: repoDir,
    env: {
      ...process.env,
      TSQ_ACTOR: actor,
    },
    stdin: stdinText === undefined ? "ignore" : "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });

  if (stdinText !== undefined) {
    const stdin = proc.stdin;
    expect(stdin).toBeDefined();
    stdin?.write(stdinText);
    stdin?.end();
  }

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

export async function runCli(
  repoDir: string,
  args: string[],
  actor = "test",
): Promise<CliResult> {
  const proc = Bun.spawn({
    cmd: ["bun", "run", cliEntry, ...args],
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
