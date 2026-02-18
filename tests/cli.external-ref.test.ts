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
  const repo = await mkdtemp(join(tmpdir(), "tasque-extref-"));
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
      TSQ_ACTOR: "extref-test",
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

describe("cli external_ref", () => {
  it("supports create/update/show roundtrip with clear path", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string; external_ref?: string } }>(
      (await runJson(repo, ["create", "Ext ref task", "--external-ref", "GH-101"])).envelope,
    ).task;
    expect(created.external_ref).toBe("GH-101");

    const updated = okData<{ task: { external_ref?: string } }>(
      (await runJson(repo, ["update", created.id, "--external-ref", "JIRA-42"])).envelope,
    ).task;
    expect(updated.external_ref).toBe("JIRA-42");

    const shown = okData<{ task: { external_ref?: string } }>(
      (await runJson(repo, ["show", created.id])).envelope,
    ).task;
    expect(shown.external_ref).toBe("JIRA-42");

    const cleared = okData<{ task: { external_ref?: string } }>(
      (await runJson(repo, ["update", created.id, "--clear-external-ref"])).envelope,
    ).task;
    expect(cleared.external_ref).toBeUndefined();
  });

  it("filters list by --external-ref exact match", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const first = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "First", "--external-ref", "GH-123"])).envelope,
    ).task;
    await runJson(repo, ["create", "Second", "--external-ref", "GH-999"]);

    const listed = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["list", "--external-ref", "GH-123"])).envelope,
    ).tasks;
    expect(listed.map((task) => task.id)).toEqual([first.id]);
  });

  it("supports external_ref fielded search", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const matched = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Search ext ref", "--external-ref", "ENG-2201"])).envelope,
    ).task;
    await runJson(repo, ["create", "Search no ref"]);

    const result = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["search", "external_ref:ENG-2201"])).envelope,
    ).tasks;
    expect(result.map((task) => task.id)).toEqual([matched.id]);
  });

  it("rejects invalid update flag combinations with external_ref", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Combo"])).envelope,
    ).task;

    const badUpdate = await runJson(repo, [
      "update",
      created.id,
      "--external-ref",
      "X-1",
      "--clear-external-ref",
    ]);
    expect(badUpdate.exitCode).toBe(1);
    expect(badUpdate.envelope.ok).toBe(false);
    expect(badUpdate.envelope.error?.code).toBe("VALIDATION_ERROR");

    const badClaim = await runJson(repo, [
      "update",
      created.id,
      "--claim",
      "--external-ref",
      "X-2",
    ]);
    expect(badClaim.exitCode).toBe(1);
    expect(badClaim.envelope.ok).toBe(false);
    expect(badClaim.envelope.error?.code).toBe("VALIDATION_ERROR");
  });
});
