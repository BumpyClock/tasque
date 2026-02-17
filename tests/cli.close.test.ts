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
  const repo = await mkdtemp(join(tmpdir(), "tasque-cli-close-"));
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
      TSQ_ACTOR: "test-close",
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

describe("cli close and reopen", () => {
  it("close sets status to closed and sets closed_at", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Task to close"])).envelope,
    ).task;

    const closed = await runJson(repo, ["close", created.id]);
    expect(closed.exitCode).toBe(0);
    expect(closed.envelope.ok).toBe(true);
    expect(closed.envelope.command).toBe("tsq close");

    const shown = okData<{ task: { id: string; status: string; closed_at?: string } }>(
      (await runJson(repo, ["show", created.id])).envelope,
    );
    expect(shown.task.status).toBe("closed");
    expect(typeof shown.task.closed_at).toBe("string");
    expect((shown.task.closed_at ?? "").length > 0).toBe(true);
  });

  it("close with reason records reason without error", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Task with reason"])).envelope,
    ).task;

    const closed = await runJson(repo, ["close", created.id, "--reason", "no longer needed"]);
    expect(closed.exitCode).toBe(0);
    expect(closed.envelope.ok).toBe(true);

    const shown = okData<{ task: { id: string; status: string } }>(
      (await runJson(repo, ["show", created.id])).envelope,
    );
    expect(shown.task.status).toBe("closed");
  });

  it("close multiple tasks at once sets all to closed", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const t1 = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Batch close task 1"])).envelope,
    ).task;
    const t2 = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Batch close task 2"])).envelope,
    ).task;
    const t3 = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Batch close task 3"])).envelope,
    ).task;

    const closed = await runJson(repo, ["close", t1.id, t2.id, t3.id]);
    expect(closed.exitCode).toBe(0);
    expect(closed.envelope.ok).toBe(true);

    const data = okData<{ tasks: Array<{ id: string; status: string }> }>(closed.envelope);
    expect(Array.isArray(data.tasks)).toBe(true);
    expect(data.tasks.length).toBe(3);

    const closedIds = data.tasks.map((task) => task.id);
    expect(closedIds.includes(t1.id)).toBe(true);
    expect(closedIds.includes(t2.id)).toBe(true);
    expect(closedIds.includes(t3.id)).toBe(true);

    for (const task of data.tasks) {
      expect(task.status).toBe("closed");
    }
  });

  it("close rejects already-closed task with VALIDATION_ERROR", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Double close task"])).envelope,
    ).task;

    const firstClose = await runJson(repo, ["close", created.id]);
    expect(firstClose.exitCode).toBe(0);

    const secondClose = await runJson(repo, ["close", created.id]);
    expect(secondClose.exitCode).toBe(1);
    expect(secondClose.envelope.ok).toBe(false);
    expect(secondClose.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("close rejects canceled task with VALIDATION_ERROR", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Canceled task"])).envelope,
    ).task;

    await runJson(repo, ["update", created.id, "--status", "canceled"]);

    const closed = await runJson(repo, ["close", created.id]);
    expect(closed.exitCode).toBe(1);
    expect(closed.envelope.ok).toBe(false);
    expect(closed.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("reopen sets status back to open and clears closed_at", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Task to reopen"])).envelope,
    ).task;

    await runJson(repo, ["close", created.id]);

    const beforeReopen = okData<{ task: { id: string; status: string; closed_at?: string } }>(
      (await runJson(repo, ["show", created.id])).envelope,
    );
    expect(beforeReopen.task.status).toBe("closed");
    expect(typeof beforeReopen.task.closed_at).toBe("string");

    const reopened = await runJson(repo, ["reopen", created.id]);
    expect(reopened.exitCode).toBe(0);
    expect(reopened.envelope.ok).toBe(true);
    expect(reopened.envelope.command).toBe("tsq reopen");

    const shown = okData<{ task: { id: string; status: string; closed_at?: unknown } }>(
      (await runJson(repo, ["show", created.id])).envelope,
    );
    expect(shown.task.status).toBe("open");
    expect(shown.task.closed_at == null).toBe(true);
  });

  it("reopen rejects a non-closed open task with VALIDATION_ERROR", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Still open task"])).envelope,
    ).task;

    const reopened = await runJson(repo, ["reopen", created.id]);
    expect(reopened.exitCode).toBe(1);
    expect(reopened.envelope.ok).toBe(false);
    expect(reopened.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("reopen rejects canceled task with VALIDATION_ERROR", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Canceled no reopen task"])).envelope,
    ).task;

    await runJson(repo, ["update", created.id, "--status", "canceled"]);

    const reopened = await runJson(repo, ["reopen", created.id]);
    expect(reopened.exitCode).toBe(1);
    expect(reopened.envelope.ok).toBe(false);
    expect(reopened.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("close and reopen roundtrip produces correct JSON envelope shape", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Roundtrip task"])).envelope,
    ).task;

    const closed = await runJson(repo, ["close", created.id, "--reason", "roundtrip test"]);
    expect(closed.exitCode).toBe(0);
    expect(closed.envelope.schema_version).toBe(1);
    expect(closed.envelope.command).toBe("tsq close");
    expect(closed.envelope.ok).toBe(true);
    const closedTask = okData<{ task: { id: string; status: string; closed_at?: string } }>(
      closed.envelope,
    ).task;
    expect(closedTask.id).toBe(created.id);
    expect(closedTask.status).toBe("closed");
    expect(typeof closedTask.closed_at).toBe("string");

    const reopened = await runJson(repo, ["reopen", created.id]);
    expect(reopened.exitCode).toBe(0);
    expect(reopened.envelope.schema_version).toBe(1);
    expect(reopened.envelope.command).toBe("tsq reopen");
    expect(reopened.envelope.ok).toBe(true);
    const reopenedTask = okData<{ task: { id: string; status: string; closed_at?: unknown } }>(
      reopened.envelope,
    ).task;
    expect(reopenedTask.id).toBe(created.id);
    expect(reopenedTask.status).toBe("open");
    expect(reopenedTask.closed_at == null).toBe(true);
  });

  it("reopen multiple tasks at once sets all back to open", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const t1 = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Batch reopen task 1"])).envelope,
    ).task;
    const t2 = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Batch reopen task 2"])).envelope,
    ).task;

    await runJson(repo, ["close", t1.id, t2.id]);

    const reopened = await runJson(repo, ["reopen", t1.id, t2.id]);
    expect(reopened.exitCode).toBe(0);
    expect(reopened.envelope.ok).toBe(true);
    expect(reopened.envelope.command).toBe("tsq reopen");

    const data = okData<{ tasks: Array<{ id: string; status: string }> }>(reopened.envelope);
    expect(Array.isArray(data.tasks)).toBe(true);
    expect(data.tasks.length).toBe(2);

    const reopenedIds = data.tasks.map((task) => task.id);
    expect(reopenedIds.includes(t1.id)).toBe(true);
    expect(reopenedIds.includes(t2.id)).toBe(true);

    for (const task of data.tasks) {
      expect(task.status).toBe("open");
    }
  });
});
