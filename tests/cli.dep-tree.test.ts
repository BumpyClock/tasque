import { afterEach, describe, expect, it } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import type { DepTreeNode } from "../src/domain/dep-tree";

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

const repos: string[] = [];
const repoRoot = resolve(import.meta.dir, "..");
const cliEntry = join(repoRoot, "src", "main.ts");

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-dep-tree-e2e-"));
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
      TSQ_ACTOR: "test-dep-tree",
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
      TSQ_ACTOR: "test-dep-tree",
    },
    stdout: "pipe",
    stderr: "pipe",
  });

  const [exitCode, stdout, stderr] = await Promise.all([
    proc.exited,
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
  ]);

  return { exitCode, stdout, stderr };
}

function okData<T>(envelope: JsonEnvelope): T {
  expect(envelope.ok).toBe(true);
  return envelope.data as T;
}

function collectIds(node: DepTreeNode): string[] {
  const ids: string[] = [node.id];
  for (const child of node.children) {
    ids.push(...collectIds(child));
  }
  return ids;
}

async function initRepo(repo: string): Promise<void> {
  const result = await runJson(repo, ["init"]);
  expect(result.exitCode).toBe(0);
}

async function createTask(repo: string, title: string): Promise<string> {
  const result = await runJson(repo, ["create", title]);
  expect(result.exitCode).toBe(0);
  const data = okData<{ task: { id: string } }>(result.envelope);
  return data.task.id;
}

async function addDep(repo: string, child: string, blocker: string): Promise<void> {
  const result = await runJson(repo, ["dep", "add", child, blocker]);
  expect(result.exitCode).toBe(0);
}

describe("cli dep tree", () => {
  it("dep tree shows blockers when direction is up", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const taskA = await createTask(repo, "Task A");
    const taskB = await createTask(repo, "Task B");
    const taskC = await createTask(repo, "Task C");

    await addDep(repo, taskA, taskB);
    await addDep(repo, taskB, taskC);

    const result = await runJson(repo, ["dep", "tree", taskA, "--direction", "up"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ root: DepTreeNode }>(result.envelope);
    const allIds = collectIds(data.root);
    expect(allIds.includes(taskB)).toBe(true);
    expect(allIds.includes(taskC)).toBe(true);
  });

  it("dep tree shows dependents when direction is down", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const taskA = await createTask(repo, "Task A");
    const taskB = await createTask(repo, "Task B");
    const taskC = await createTask(repo, "Task C");

    await addDep(repo, taskA, taskB);
    await addDep(repo, taskB, taskC);

    const result = await runJson(repo, ["dep", "tree", taskC, "--direction", "down"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ root: DepTreeNode }>(result.envelope);
    const allIds = collectIds(data.root);
    expect(allIds.includes(taskB)).toBe(true);
    expect(allIds.includes(taskA)).toBe(true);
  });

  it("dep tree shows both blockers and dependents when direction is both", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const taskA = await createTask(repo, "Task A");
    const taskB = await createTask(repo, "Task B");
    const taskC = await createTask(repo, "Task C");

    await addDep(repo, taskA, taskB);
    await addDep(repo, taskB, taskC);

    const result = await runJson(repo, ["dep", "tree", taskB, "--direction", "both"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ root: DepTreeNode }>(result.envelope);
    expect(data.root.id).toBe(taskB);
    const allIds = collectIds(data.root);
    expect(allIds.includes(taskA)).toBe(true);
    expect(allIds.includes(taskC)).toBe(true);
  });

  it("dep tree respects depth limit and only shows immediate neighbors at depth 1", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const taskA = await createTask(repo, "Task A");
    const taskB = await createTask(repo, "Task B");
    const taskC = await createTask(repo, "Task C");
    const taskD = await createTask(repo, "Task D");

    await addDep(repo, taskA, taskB);
    await addDep(repo, taskB, taskC);
    await addDep(repo, taskC, taskD);

    const result = await runJson(repo, ["dep", "tree", taskA, "--direction", "up", "--depth", "1"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ root: DepTreeNode }>(result.envelope);
    const allIds = collectIds(data.root);
    expect(allIds.includes(taskB)).toBe(true);
    expect(allIds.includes(taskC)).toBe(false);
    expect(allIds.includes(taskD)).toBe(false);
  });

  it("dep tree returns empty children for a task with no deps", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const taskA = await createTask(repo, "Standalone task");

    const result = await runJson(repo, ["dep", "tree", taskA, "--direction", "both"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ root: DepTreeNode }>(result.envelope);
    expect(data.root.id).toBe(taskA);
    expect(data.root.children).toBeArray();
    expect(data.root.children.length).toBe(0);
  });

  it("dep tree handles cycles safely without hanging", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const taskA = await createTask(repo, "Cycle A");
    const taskB = await createTask(repo, "Cycle B");

    await addDep(repo, taskA, taskB);

    const result = await runJson(repo, ["dep", "tree", taskA, "--direction", "both"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ root: DepTreeNode }>(result.envelope);
    expect(data.root.id).toBe(taskA);
    const allIds = collectIds(data.root);
    expect(allIds.includes(taskB)).toBe(true);
  });

  it("dep tree returns NOT_FOUND error for unknown task id", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const result = await runJson(repo, ["dep", "tree", "tsq-doesnotexist"]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("TASK_NOT_FOUND");
  });

  it("dep tree JSON envelope has correct root node shape", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const taskA = await createTask(repo, "Shape check task");
    const taskB = await createTask(repo, "Shape check blocker");
    await addDep(repo, taskA, taskB);

    const result = await runJson(repo, ["dep", "tree", taskA, "--direction", "up"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ root: DepTreeNode }>(result.envelope);

    expect(data.root).toBeObject();
    expect(typeof data.root.id).toBe("string");
    expect(typeof data.root.direction).toBe("string");
    expect(typeof data.root.depth).toBe("number");
    expect(data.root.depth).toBe(0);
    expect(Array.isArray(data.root.children)).toBe(true);
    expect(data.root.task).toBeObject();
    expect(typeof data.root.task.id).toBe("string");
    expect(typeof data.root.task.title).toBe("string");
    expect(typeof data.root.task.status).toBe("string");

    expect(data.root.children.length).toBeGreaterThan(0);
    const firstChild = data.root.children[0];
    expect(firstChild).toBeDefined();
    if (firstChild) {
      expect(typeof firstChild.id).toBe("string");
      expect(firstChild.depth).toBe(1);
      expect(Array.isArray(firstChild.children)).toBe(true);
    }
  });

  it("dep tree uses both direction when no direction flag is provided", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const taskA = await createTask(repo, "Default dir A");
    const taskB = await createTask(repo, "Default dir B");
    const taskC = await createTask(repo, "Default dir C");

    await addDep(repo, taskA, taskB);
    await addDep(repo, taskB, taskC);

    const result = await runJson(repo, ["dep", "tree", taskB]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ root: DepTreeNode }>(result.envelope);
    expect(data.root.direction).toBe("both");
    const allIds = collectIds(data.root);
    expect(allIds.includes(taskA)).toBe(true);
    expect(allIds.includes(taskC)).toBe(true);
  });

  it("dep tree correctly handles diamond dependency where multiple paths converge", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const taskA = await createTask(repo, "Diamond A");
    const taskB = await createTask(repo, "Diamond B");
    const taskC = await createTask(repo, "Diamond C");
    const taskD = await createTask(repo, "Diamond D");

    await addDep(repo, taskA, taskB);
    await addDep(repo, taskA, taskC);
    await addDep(repo, taskB, taskD);
    await addDep(repo, taskC, taskD);

    const result = await runJson(repo, ["dep", "tree", taskA, "--direction", "up"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ root: DepTreeNode }>(result.envelope);
    const allIds = collectIds(data.root);
    expect(allIds.includes(taskB)).toBe(true);
    expect(allIds.includes(taskC)).toBe(true);
    expect(allIds.includes(taskD)).toBe(true);
  });

  it("dep tree human readable output includes task ids for direction up", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const taskA = await createTask(repo, "Human A");
    const taskB = await createTask(repo, "Human B");
    await addDep(repo, taskA, taskB);

    const result = await runCli(repo, ["dep", "tree", taskA, "--direction", "up"]);
    expect(result.exitCode).toBe(0);
    expect(result.stdout.includes(taskA)).toBe(true);
    expect(result.stdout.includes(taskB)).toBe(true);
  });
});
