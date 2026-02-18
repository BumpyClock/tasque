import { afterEach, describe, expect, it } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

interface JsonEnvelope {
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

interface JsonResult {
  exitCode: number;
  stdout: string;
  stderr: string;
  envelope: JsonEnvelope;
}

const repos: string[] = [];
const repoRoot = resolve(import.meta.dir, "..");
const cliEntry = join(repoRoot, "src", "main.ts");

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-stale-e2e-"));
  repos.push(repo);
  return repo;
}

afterEach(async () => {
  await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
});

function assertEnvelopeShape(value: unknown): asserts value is JsonEnvelope {
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

async function runJson(repoDir: string, args: string[]): Promise<JsonResult> {
  const proc = Bun.spawn({
    cmd: ["bun", "run", cliEntry, ...args, "--json"],
    cwd: repoDir,
    env: {
      ...process.env,
      TSQ_ACTOR: "test-stale",
    },
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

function okData<T>(envelope: JsonEnvelope): T {
  expect(envelope.ok).toBe(true);
  return envelope.data as T;
}

describe("cli stale", () => {
  it("returns stale json contract with defaults", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    await runJson(repo, ["create", "Recent task"]);

    const stale = await runJson(repo, ["stale"]);
    expect(stale.exitCode).toBe(0);
    expect(stale.envelope.command).toBe("tsq stale");

    const data = okData<{
      tasks: Array<{ id: string }>;
      days: number;
      cutoff: string;
      statuses: string[];
    }>(stale.envelope);
    expect(Array.isArray(data.tasks)).toBe(true);
    expect(data.days).toBe(30);
    expect(typeof data.cutoff).toBe("string");
    expect(data.statuses).toEqual(["open", "in_progress", "blocked"]);
  });

  it("uses default status scope open/in_progress/blocked", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const openTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Open stale default"])).envelope,
    ).task;
    const inProgressTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "In progress stale default"])).envelope,
    ).task;
    const blockedTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Blocked stale default"])).envelope,
    ).task;
    const closedTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Closed stale default"])).envelope,
    ).task;

    await runJson(repo, ["update", inProgressTask.id, "--status", "in_progress"]);
    await runJson(repo, ["update", blockedTask.id, "--status", "blocked"]);
    await runJson(repo, ["update", closedTask.id, "--status", "closed"]);

    const stale = await runJson(repo, ["stale", "--days", "0"]);
    expect(stale.exitCode).toBe(0);
    const ids = okData<{ tasks: Array<{ id: string }> }>(stale.envelope).tasks.map(
      (task) => task.id,
    );

    expect(ids.includes(openTask.id)).toBe(true);
    expect(ids.includes(inProgressTask.id)).toBe(true);
    expect(ids.includes(blockedTask.id)).toBe(true);
    expect(ids.includes(closedTask.id)).toBe(false);
  });

  it("overrides status scope and supports done alias with optional assignee filter", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const bobTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Closed stale bob"])).envelope,
    ).task;
    const aliceTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Closed stale alice"])).envelope,
    ).task;

    await runJson(repo, ["update", bobTask.id, "--claim", "--assignee", "bob"]);
    await runJson(repo, ["update", aliceTask.id, "--claim", "--assignee", "alice"]);
    await runJson(repo, ["close", bobTask.id]);
    await runJson(repo, ["close", aliceTask.id]);

    const stale = await runJson(repo, [
      "stale",
      "--days",
      "0",
      "--status",
      "done",
      "--assignee",
      "bob",
    ]);
    expect(stale.exitCode).toBe(0);
    const data = okData<{ tasks: Array<{ id: string }>; statuses: string[] }>(stale.envelope);
    expect(data.statuses).toEqual(["closed"]);
    expect(data.tasks.map((task) => task.id)).toEqual([bobTask.id]);
  });

  it("validates days as integer >= 0", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const negative = await runJson(repo, ["stale", "--days", "-1"]);
    expect(negative.exitCode).toBe(1);
    expect(negative.envelope.ok).toBe(false);
    expect(negative.envelope.error?.code).toBe("VALIDATION_ERROR");

    const notInteger = await runJson(repo, ["stale", "--days", "1.5"]);
    expect(notInteger.exitCode).toBe(1);
    expect(notInteger.envelope.ok).toBe(false);
    expect(notInteger.envelope.error?.code).toBe("VALIDATION_ERROR");
  });
});
