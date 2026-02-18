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

interface CliResult {
  exitCode: number;
  stdout: string;
  stderr: string;
}

interface TaskNote {
  event_id: string;
  ts: string;
  actor: string;
  text: string;
}

const repos: string[] = [];
const repoRoot = resolve(import.meta.dir, "..");
const cliEntry = join(repoRoot, "src", "main.ts");

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-rich-content-"));
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
  actor = "test-rich-content",
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

async function runCli(repoDir: string, args: string[]): Promise<CliResult> {
  const proc = Bun.spawn({
    cmd: ["bun", "run", cliEntry, ...args],
    cwd: repoDir,
    env: {
      ...process.env,
      TSQ_ACTOR: "test-rich-content",
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

function okData<T>(envelope: JsonEnvelope): T {
  expect(envelope.ok).toBe(true);
  return envelope.data as T;
}

describe("cli rich content", () => {
  it("create stores description and show returns empty notes list", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{
      task: { id: string; description?: string; notes: TaskNote[] };
    }>(
      (await runJson(repo, ["create", "Rich task", "--description", "Capture rollout context"]))
        .envelope,
    ).task;

    expect(created.description).toBe("Capture rollout context");
    expect(created.notes).toEqual([]);

    const shown = okData<{
      task: { id: string; description?: string; notes: TaskNote[] };
    }>((await runJson(repo, ["show", created.id])).envelope);
    expect(shown.task.description).toBe("Capture rollout context");
    expect(shown.task.notes).toEqual([]);
  });

  it("update sets and clears description", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Description flow"])).envelope,
    ).task;

    const updated = await runJson(repo, [
      "update",
      created.id,
      "--description",
      "Detailed implementation note",
    ]);
    expect(updated.exitCode).toBe(0);

    const shownWithDescription = okData<{ task: { description?: string } }>(
      (await runJson(repo, ["show", created.id])).envelope,
    );
    expect(shownWithDescription.task.description).toBe("Detailed implementation note");

    const cleared = await runJson(repo, ["update", created.id, "--clear-description"]);
    expect(cleared.exitCode).toBe(0);

    const shownCleared = okData<{ task: { description?: string } }>(
      (await runJson(repo, ["show", created.id])).envelope,
    );
    expect(shownCleared.task.description).toBeUndefined();
  });

  it("rejects combining --description with --clear-description", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Conflict task"])).envelope,
    ).task;

    const result = await runJson(repo, [
      "update",
      created.id,
      "--description",
      "A",
      "--clear-description",
    ]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("keeps claim mode exclusive from description options", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Claim conflict task"])).envelope,
    ).task;

    const result = await runJson(repo, [
      "update",
      created.id,
      "--claim",
      "--description",
      "should fail",
    ]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("note add appends deterministic metadata and note list returns all entries", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Note flow"])).envelope,
    ).task;

    const first = await runJson(repo, ["note", "add", created.id, "First note body"], "actor-one");
    expect(first.exitCode).toBe(0);
    const firstData = okData<{ task_id: string; note: TaskNote; notes_count: number }>(
      first.envelope,
    );
    expect(firstData.task_id).toBe(created.id);
    expect(firstData.note.actor).toBe("actor-one");
    expect(firstData.note.text).toBe("First note body");
    expect(firstData.note.event_id.length > 0).toBe(true);
    expect(firstData.note.ts.length > 0).toBe(true);
    expect(firstData.notes_count).toBe(1);

    await runJson(repo, ["note", "add", created.id, "Second note body"], "actor-two");

    const listed = await runJson(repo, ["note", "list", created.id]);
    expect(listed.exitCode).toBe(0);
    const listData = okData<{ task_id: string; notes: TaskNote[] }>(listed.envelope);
    expect(listData.task_id).toBe(created.id);
    expect(listData.notes.length).toBe(2);
    expect(listData.notes[0]?.text).toBe("First note body");
    expect(listData.notes[1]?.text).toBe("Second note body");
    expect(listData.notes[1]?.actor).toBe("actor-two");
  });

  it("show human render includes description and notes visibility", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (
        await runJson(repo, [
          "create",
          "Human render rich task",
          "--description",
          "Visible description text",
        ])
      ).envelope,
    ).task;
    await runJson(repo, ["note", "add", created.id, "Visible note"]);

    const shown = await runCli(repo, ["show", created.id]);
    expect(shown.exitCode).toBe(0);
    expect(shown.stdout.includes("description=Visible description text")).toBe(true);
    expect(shown.stdout.includes("notes=1")).toBe(true);
  });
});
