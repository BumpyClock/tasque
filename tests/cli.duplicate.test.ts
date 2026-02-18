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
  const repo = await mkdtemp(join(tmpdir(), "tasque-duplicate-"));
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

async function runJson(
  repoDir: string,
  args: string[],
  actor = "duplicate-test",
): Promise<JsonResult> {
  const proc = Bun.spawn({
    cmd: ["bun", "run", cliEntry, ...args, "--json"],
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

describe("cli duplicate workflow", () => {
  it("duplicate closes source, sets duplicate_of, and keeps dependencies unchanged", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const blocker = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Blocker task"])).envelope,
    ).task;
    const canonical = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Canonical task"])).envelope,
    ).task;
    const duplicate = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Duplicate task"])).envelope,
    ).task;

    await runJson(repo, ["dep", "add", duplicate.id, blocker.id]);

    const marked = await runJson(repo, [
      "duplicate",
      duplicate.id,
      "--of",
      canonical.id,
      "--reason",
      "same implementation target",
    ]);
    expect(marked.exitCode).toBe(0);
    const task = okData<{
      task: { id: string; status: string; duplicate_of?: string; closed_at?: string };
    }>(marked.envelope).task;
    expect(task.id).toBe(duplicate.id);
    expect(task.status).toBe("closed");
    expect(task.duplicate_of).toBe(canonical.id);
    expect(typeof task.closed_at).toBe("string");

    const shown = okData<{
      blockers: string[];
      links: Record<string, string[]>;
    }>((await runJson(repo, ["show", duplicate.id])).envelope);
    expect(shown.blockers).toEqual([blocker.id]);
    expect(shown.links.duplicates).toEqual([canonical.id]);
  });

  it("rejects duplicate self-edge", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const task = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Self"])).envelope,
    ).task;

    const result = await runJson(repo, ["duplicate", task.id, "--of", task.id]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("rejects duplicate cycle chains", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const alpha = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Alpha"])).envelope,
    ).task;
    const beta = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Beta"])).envelope,
    ).task;

    const first = await runJson(repo, ["duplicate", alpha.id, "--of", beta.id]);
    expect(first.exitCode).toBe(0);

    const second = await runJson(repo, ["duplicate", beta.id, "--of", alpha.id]);
    expect(second.exitCode).toBe(1);
    expect(second.envelope.ok).toBe(false);
    expect(second.envelope.error?.code).toBe("DUPLICATE_CYCLE");
  });

  it("rejects canceled canonical targets", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const source = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Source"])).envelope,
    ).task;
    const canonical = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Canonical"])).envelope,
    ).task;

    await runJson(repo, ["update", canonical.id, "--status", "canceled"]);
    const result = await runJson(repo, ["duplicate", source.id, "--of", canonical.id]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("INVALID_STATUS");
  });

  it("duplicates dry-run scaffold groups active tasks by normalized title", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const first = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Parser cleanup"])).envelope,
    ).task;
    const second = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "parser cleanup!!"])).envelope,
    ).task;
    await runJson(repo, ["create", "Unrelated item"]);

    const result = await runJson(repo, ["duplicates", "--limit", "10"]);
    expect(result.exitCode).toBe(0);
    expect(result.envelope.command).toBe("tsq duplicates");
    const data = okData<{
      scanned: number;
      groups: Array<{ key: string; tasks: Array<{ id: string }> }>;
    }>(result.envelope);
    expect(data.scanned).toBeGreaterThanOrEqual(3);
    expect(data.groups.length).toBeGreaterThanOrEqual(1);
    const parserGroup = data.groups.find((group) => group.key === "parser cleanup");
    expect(parserGroup).toBeDefined();
    expect(parserGroup?.tasks.map((task) => task.id)).toEqual([first.id, second.id]);
  });
});
