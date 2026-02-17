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

interface EventRecord {
  event_id: string;
  ts: string;
  actor: string;
  type: string;
  task_id: string;
  payload: Record<string, unknown>;
}

interface HistoryData {
  events: EventRecord[];
  count: number;
  truncated: boolean;
}

const repos: string[] = [];
const repoRoot = resolve(import.meta.dir, "..");
const cliEntry = join(repoRoot, "src", "main.ts");

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-history-e2e-"));
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
  actor = "test-history",
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

describe("cli history e2e", () => {
  it("history shows task creation event", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "History creation test"])).envelope,
    ).task;

    const result = await runJson(repo, ["history", created.id]);
    expect(result.exitCode).toBe(0);

    const data = okData<HistoryData>(result.envelope);
    expect(Array.isArray(data.events)).toBe(true);
    expect(data.events.length).toBeGreaterThanOrEqual(1);

    const creationEvent = data.events.find((event) => event.type === "task.created");
    expect(creationEvent).toBeDefined();
    expect(creationEvent?.task_id).toBe(created.id);
  });

  it("history shows update events in newest-first order", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "History updates test"])).envelope,
    ).task;

    await runJson(repo, ["update", created.id, "--title", "Updated title"]);
    await runJson(repo, ["update", created.id, "--status", "in_progress"]);

    const result = await runJson(repo, ["history", created.id]);
    expect(result.exitCode).toBe(0);

    const data = okData<HistoryData>(result.envelope);
    expect(data.events.length).toBeGreaterThanOrEqual(3);

    const types = data.events.map((event) => event.type);
    expect(types).toContain("task.created");
    expect(types).toContain("task.updated");

    for (let idx = 0; idx < data.events.length - 1; idx += 1) {
      const current = data.events[idx];
      const next = data.events[idx + 1];
      expect((current?.ts ?? "") >= (next?.ts ?? "")).toBe(true);
    }
  });

  it("history filters by event type", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "History type filter test"])).envelope,
    ).task;

    await runJson(repo, ["update", created.id, "--status", "in_progress"]);
    await runJson(repo, ["update", created.id, "--claim"]);

    const result = await runJson(repo, ["history", created.id, "--type", "task.claimed"]);
    expect(result.exitCode).toBe(0);

    const data = okData<HistoryData>(result.envelope);
    expect(data.events.length).toBeGreaterThanOrEqual(1);
    for (const event of data.events) {
      expect(event.type).toBe("task.claimed");
    }
  });

  it("history filters by actor", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"], "actor-alpha");

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "History actor filter test"], "actor-alpha")).envelope,
    ).task;

    await runJson(repo, ["update", created.id, "--title", "Updated by beta"], "actor-beta");

    const result = await runJson(repo, ["history", created.id, "--actor", "actor-alpha"]);
    expect(result.exitCode).toBe(0);

    const data = okData<HistoryData>(result.envelope);
    expect(data.events.length).toBeGreaterThanOrEqual(1);
    for (const event of data.events) {
      expect(event.actor).toBe("actor-alpha");
    }

    const betaEvents = data.events.filter((event) => event.actor === "actor-beta");
    expect(betaEvents.length).toBe(0);
  });

  it("history limits results and reports truncated when limit is reached", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "History limit test"])).envelope,
    ).task;

    await runJson(repo, ["update", created.id, "--title", "Update 1"]);
    await runJson(repo, ["update", created.id, "--title", "Update 2"]);
    await runJson(repo, ["update", created.id, "--title", "Update 3"]);
    await runJson(repo, ["update", created.id, "--title", "Update 4"]);

    const result = await runJson(repo, ["history", created.id, "--limit", "2"]);
    expect(result.exitCode).toBe(0);

    const data = okData<HistoryData>(result.envelope);
    expect(data.events.length).toBe(2);
    expect(data.truncated).toBe(true);
    expect(data.count).toBe(2);
  });

  it("history filters by since date and excludes earlier events", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "History since filter test"])).envelope,
    ).task;

    const sinceTimestamp = new Date().toISOString();

    await runJson(repo, ["update", created.id, "--title", "After since timestamp"]);

    const result = await runJson(repo, ["history", created.id, "--since", sinceTimestamp]);
    expect(result.exitCode).toBe(0);

    const data = okData<HistoryData>(result.envelope);
    const newerEvents = data.events.filter((event) => event.ts >= sinceTimestamp);
    expect(newerEvents.length).toBeGreaterThanOrEqual(1);

    for (const event of data.events) {
      expect(event.ts >= sinceTimestamp).toBe(true);
    }
  });

  it("history returns NOT_FOUND error for unknown task id", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const result = await runJson(repo, ["history", "tsq-nonexistent999"]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("TASK_NOT_FOUND");
  });

  it("history includes dependency events involving the task", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const taskA = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "History dep task A"])).envelope,
    ).task;
    const taskB = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "History dep task B"])).envelope,
    ).task;

    await runJson(repo, ["dep", "add", taskA.id, taskB.id]);
    await runJson(repo, ["dep", "remove", taskA.id, taskB.id]);

    const result = await runJson(repo, ["history", taskA.id]);
    expect(result.exitCode).toBe(0);

    const data = okData<HistoryData>(result.envelope);
    const types = data.events.map((event) => event.type);
    expect(types).toContain("dep.added");
    expect(types).toContain("dep.removed");
  });

  it("history JSON envelope has correct schema shape", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "History envelope shape test"])).envelope,
    ).task;

    const result = await runJson(repo, ["history", created.id]);
    expect(result.exitCode).toBe(0);

    const envelope = result.envelope;
    expect(envelope.schema_version).toBe(1);
    expect(typeof envelope.command).toBe("string");
    expect(envelope.ok).toBe(true);

    const data = okData<HistoryData>(envelope);
    expect(Array.isArray(data.events)).toBe(true);
    expect(typeof data.count).toBe("number");
    expect(typeof data.truncated).toBe("boolean");

    for (const event of data.events) {
      expect(typeof event.event_id).toBe("string");
      expect(typeof event.ts).toBe("string");
      expect(typeof event.actor).toBe("string");
      expect(typeof event.type).toBe("string");
      expect(typeof event.task_id).toBe("string");
      expect(event.payload).toBeObject();
    }
  });

  it("history truncated is false when event count is below default limit", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "History below limit test"])).envelope,
    ).task;

    await runJson(repo, ["update", created.id, "--title", "One update"]);

    const result = await runJson(repo, ["history", created.id]);
    expect(result.exitCode).toBe(0);

    const data = okData<HistoryData>(result.envelope);
    expect(data.events.length).toBeLessThan(50);
    expect(data.truncated).toBe(false);
    expect(data.count).toBe(data.events.length);
  });
});
