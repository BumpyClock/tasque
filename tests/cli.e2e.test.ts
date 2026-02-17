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
  const repo = await mkdtemp(join(tmpdir(), "tasque-cli-e2e-"));
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
      TSQ_ACTOR: "task4-e2e",
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

describe("cli e2e", () => {
  it("covers init/create/show/list/ready happy path", async () => {
    const repo = await makeRepo();

    const init = await runJson(repo, ["init"]);
    expect(init.exitCode).toBe(0);
    expect(init.envelope.ok).toBe(true);

    const created = await runJson(repo, ["create", "Task 4 happy path"]);
    expect(created.exitCode).toBe(0);
    const createdTask = okData<{ task: { id: string; title: string } }>(created.envelope).task;
    expect(createdTask.title).toBe("Task 4 happy path");

    const shown = await runJson(repo, ["show", createdTask.id]);
    expect(shown.exitCode).toBe(0);
    const shownData = okData<{ task: { id: string } }>(shown.envelope);
    expect(shownData.task.id).toBe(createdTask.id);

    const listed = await runJson(repo, ["list"]);
    expect(listed.exitCode).toBe(0);
    const listedIds = okData<{ tasks: Array<{ id: string }> }>(listed.envelope).tasks.map(
      (task) => task.id,
    );
    expect(listedIds.includes(createdTask.id)).toBe(true);

    const ready = await runJson(repo, ["ready"]);
    expect(ready.exitCode).toBe(0);
    const readyIds = okData<{ tasks: Array<{ id: string }> }>(ready.envelope).tasks.map(
      (task) => task.id,
    );
    expect(readyIds.includes(createdTask.id)).toBe(true);
  });

  it("removes blocked task from ready list after dep add", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const child = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Child task"])).envelope,
    ).task;
    const blocker = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Blocker task"])).envelope,
    ).task;

    const readyBeforeIds = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["ready"])).envelope,
    ).tasks.map((task) => task.id);
    expect(readyBeforeIds.includes(child.id)).toBe(true);

    const depAdd = await runJson(repo, ["dep", "add", child.id, blocker.id]);
    expect(depAdd.exitCode).toBe(0);
    expect(depAdd.envelope.ok).toBe(true);

    const readyAfterIds = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["ready"])).envelope,
    ).tasks.map((task) => task.id);
    expect(readyAfterIds.includes(child.id)).toBe(false);
    expect(readyAfterIds.includes(blocker.id)).toBe(true);
  });

  it("supersede closes source task and sets superseded_by", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const oldTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Old task"])).envelope,
    ).task;
    const replacement = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Replacement task"])).envelope,
    ).task;

    const superseded = await runJson(repo, [
      "supersede",
      oldTask.id,
      "--with",
      replacement.id,
      "--reason",
      "obsolete",
    ]);
    expect(superseded.exitCode).toBe(0);
    const supersededTask = okData<{
      task: { id: string; status: string; superseded_by?: string; closed_at?: string };
    }>(superseded.envelope).task;
    expect(supersededTask.id).toBe(oldTask.id);
    expect(supersededTask.status).toBe("closed");
    expect(supersededTask.superseded_by).toBe(replacement.id);
    expect(typeof supersededTask.closed_at).toBe("string");

    const shown = okData<{
      task: { status: string; superseded_by?: string };
    }>((await runJson(repo, ["show", oldTask.id])).envelope);
    expect(shown.task.status).toBe("closed");
    expect(shown.task.superseded_by).toBe(replacement.id);
  });

  it("returns ambiguous error for non-unique partial ID", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    await runJson(repo, ["create", "Alpha"]);
    await runJson(repo, ["create", "Beta"]);

    const ambiguous = await runJson(repo, ["show", "tsq-"]);
    expect(ambiguous.exitCode).toBe(1);
    expect(ambiguous.envelope.ok).toBe(false);
    expect(ambiguous.envelope.error?.code).toBe("TASK_ID_AMBIGUOUS");
    expect(ambiguous.envelope.error?.details).toBeObject();

    const details = ambiguous.envelope.error?.details as { candidates?: string[] } | undefined;
    expect(Array.isArray(details?.candidates)).toBe(true);
    expect((details?.candidates?.length ?? 0) >= 2).toBe(true);
  });
});
