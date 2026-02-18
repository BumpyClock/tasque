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
  };
}

interface WatchFrameData {
  frame_ts: string;
  interval_s: number;
  filters: { status: string[]; assignee?: string };
  summary: { total: number; open: number; in_progress: number; blocked: number };
  tasks: Array<{ id: string; status: string; priority: number; title: string }>;
}

const repos: string[] = [];
const repoRoot = resolve(import.meta.dir, "..");
const cliEntry = join(repoRoot, "src", "main.ts");

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-watch-"));
  repos.push(repo);
  return repo;
}

afterEach(async () => {
  await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
});

async function runCli(
  repoDir: string,
  args: string[],
): Promise<{ exitCode: number; stdout: string; stderr: string }> {
  const proc = Bun.spawn({
    cmd: ["bun", "run", cliEntry, ...args],
    cwd: repoDir,
    env: { ...process.env, TSQ_ACTOR: "watch-test" },
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

async function runJson(
  repoDir: string,
  args: string[],
): Promise<{ exitCode: number; envelope: JsonEnvelope }> {
  const result = await runCli(repoDir, [...args, "--json"]);
  const envelope = JSON.parse(result.stdout.trim()) as JsonEnvelope;
  return { exitCode: result.exitCode, envelope };
}

async function initRepo(repoDir: string): Promise<void> {
  await runCli(repoDir, ["init"]);
}

async function createTask(repoDir: string, title: string, opts: string[] = []): Promise<string> {
  const result = await runJson(repoDir, ["create", title, ...opts]);
  const data = (result.envelope as { data?: { task?: { id?: string } } }).data;
  return data?.task?.id ?? "";
}

describe("tsq watch", () => {
  it("--once shows empty state when no tasks", async () => {
    const repo = await makeRepo();
    await initRepo(repo);
    const result = await runCli(repo, ["watch", "--once"]);
    expect(result.exitCode).toBe(0);
    expect(result.stdout).toContain("[tsq watch]");
    expect(result.stdout).toContain("active=0");
    expect(result.stdout).toContain("no active tasks");
  });

  it("--once shows active tasks", async () => {
    const repo = await makeRepo();
    await initRepo(repo);
    await createTask(repo, "Task Alpha", ["-p", "1"]);
    await createTask(repo, "Task Beta", ["-p", "2"]);

    const result = await runCli(repo, ["watch", "--once"]);
    expect(result.exitCode).toBe(0);
    expect(result.stdout).toContain("active=2");
    expect(result.stdout).toContain("Task Alpha");
    expect(result.stdout).toContain("Task Beta");
  });

  it("--once --json produces valid envelope", async () => {
    const repo = await makeRepo();
    await initRepo(repo);
    await createTask(repo, "JSON test task");

    const { exitCode, envelope } = await runJson(repo, ["watch", "--once"]);
    expect(exitCode).toBe(0);
    expect(envelope.schema_version).toBe(1);
    expect(envelope.command).toBe("tsq watch");
    expect(envelope.ok).toBe(true);

    const data = envelope.data as WatchFrameData;
    expect(data.frame_ts).toBeTruthy();
    expect(data.interval_s).toBe(30);
    expect(data.filters.status).toEqual(["open", "in_progress"]);
    expect(data.summary.total).toBe(1);
    expect(data.summary.open).toBe(1);
    expect(data.tasks).toHaveLength(1);
    expect(data.tasks[0]?.title).toBe("JSON test task");
  });

  it("--once --json with empty store", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const { exitCode, envelope } = await runJson(repo, ["watch", "--once"]);
    expect(exitCode).toBe(0);
    expect(envelope.ok).toBe(true);
    const data = envelope.data as WatchFrameData;
    expect(data.summary.total).toBe(0);
    expect(data.tasks).toHaveLength(0);
  });

  it("orders in_progress before open, then by priority", async () => {
    const repo = await makeRepo();
    await initRepo(repo);
    await createTask(repo, "Low priority open", ["-p", "2"]);
    await createTask(repo, "High priority open", ["-p", "0"]);
    const id3 = await createTask(repo, "In progress task", ["-p", "1"]);
    await runCli(repo, ["update", id3, "--status", "in_progress"]);

    const { envelope } = await runJson(repo, ["watch", "--once"]);
    const data = envelope.data as WatchFrameData;
    expect(data.tasks[0]?.title).toBe("In progress task");
    expect(data.tasks[1]?.title).toBe("High priority open");
    expect(data.tasks[2]?.title).toBe("Low priority open");
  });

  it("filters by --status", async () => {
    const repo = await makeRepo();
    await initRepo(repo);
    await createTask(repo, "Open task");
    const id2 = await createTask(repo, "Will be in progress");
    await runCli(repo, ["update", id2, "--status", "in_progress"]);

    const { envelope } = await runJson(repo, ["watch", "--once", "--status", "in_progress"]);
    const data = envelope.data as WatchFrameData;
    expect(data.tasks).toHaveLength(1);
    expect(data.tasks[0]?.title).toBe("Will be in progress");
    expect(data.filters.status).toEqual(["in_progress"]);
  });

  it("filters by --assignee", async () => {
    const repo = await makeRepo();
    await initRepo(repo);
    await createTask(repo, "Unassigned task");
    const id2 = await createTask(repo, "Assigned task");
    await runCli(repo, ["update", id2, "--claim", "--assignee", "alice"]);

    const { envelope } = await runJson(repo, ["watch", "--once", "--assignee", "alice"]);
    const data = envelope.data as WatchFrameData;
    expect(data.tasks).toHaveLength(1);
    expect(data.tasks[0]?.title).toBe("Assigned task");
    expect(data.filters.assignee).toBe("alice");
  });

  it("--once --tree shows hierarchy", async () => {
    const repo = await makeRepo();
    await initRepo(repo);
    const parentId = await createTask(repo, "Parent feature", ["--kind", "feature"]);
    await createTask(repo, "Child task", ["--parent", parentId]);

    const result = await runCli(repo, ["watch", "--once", "--tree"]);
    expect(result.exitCode).toBe(0);
    expect(result.stdout).toContain("Parent feature");
    expect(result.stdout).toContain("Child task");
    // Tree connectors
    expect(result.stdout).toMatch(/[├└]/);
  });

  it("rejects invalid interval", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const result = await runCli(repo, ["watch", "--once", "--interval", "0"]);
    expect(result.exitCode).toBe(1);
  });

  it("rejects interval > 60", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const result = await runCli(repo, ["watch", "--once", "--interval", "61"]);
    expect(result.exitCode).toBe(1);
  });

  it("excludes closed/canceled from default filter", async () => {
    const repo = await makeRepo();
    await initRepo(repo);
    await createTask(repo, "Active task");
    const id2 = await createTask(repo, "Closed task");
    await runCli(repo, ["update", id2, "--status", "closed"]);

    const { envelope } = await runJson(repo, ["watch", "--once"]);
    const data = envelope.data as WatchFrameData;
    expect(data.tasks).toHaveLength(1);
    expect(data.tasks[0]?.title).toBe("Active task");
  });

  it("summary counts are correct", async () => {
    const repo = await makeRepo();
    await initRepo(repo);
    await createTask(repo, "Open 1");
    await createTask(repo, "Open 2");
    const id3 = await createTask(repo, "In progress");
    await runCli(repo, ["update", id3, "--status", "in_progress"]);

    const { envelope } = await runJson(repo, ["watch", "--once"]);
    const data = envelope.data as WatchFrameData;
    expect(data.summary.total).toBe(3);
    expect(data.summary.open).toBe(2);
    expect(data.summary.in_progress).toBe(1);
    expect(data.summary.blocked).toBe(0);
  });

  it("header contains expected metadata", async () => {
    const repo = await makeRepo();
    await initRepo(repo);
    await createTask(repo, "Test task");

    const result = await runCli(repo, ["watch", "--once", "--interval", "5"]);
    expect(result.stdout).toContain("[tsq watch]");
    expect(result.stdout).toContain("interval=5s");
    expect(result.stdout).toContain("refreshed=");
  });

  it("custom interval is reflected in JSON", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const { envelope } = await runJson(repo, ["watch", "--once", "--interval", "10"]);
    const data = envelope.data as WatchFrameData;
    expect(data.interval_s).toBe(10);
  });
});
