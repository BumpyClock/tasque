import { afterEach, describe, expect, it } from "bun:test";
import { SCHEMA_VERSION } from "../src/types";
import {
  cleanupRepos,
  makeRepo as makeRepoBase,
  runCli as runCliBase,
  runJson as runJsonBase,
} from "./helpers";

interface WatchFrameData {
  frame_ts: string;
  interval_s: number;
  filters: { status: string[]; assignee?: string };
  summary: { total: number; open: number; in_progress: number; blocked: number };
  tasks: Array<{ id: string; status: string; priority: number; title: string }>;
}

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-watch-");
}

afterEach(cleanupRepos);

async function runCli(repoDir: string, args: string[]) {
  return runCliBase(repoDir, args, "watch-test");
}

async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "watch-test");
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
    expect(envelope.schema_version).toBe(SCHEMA_VERSION);
    expect(envelope.command).toBe("tsq watch");
    expect(envelope.ok).toBe(true);

    const data = envelope.data as WatchFrameData;
    expect(data.frame_ts).toBeTruthy();
    expect(data.interval_s).toBe(2);
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

  it("--once --json validation error produces consistent error envelope", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const { exitCode, envelope } = await runJson(repo, ["watch", "--once", "--interval", "0"]);
    expect(exitCode).toBe(1);
    expect(envelope.ok).toBe(false);
    expect(envelope.error?.code).toBe("VALIDATION_ERROR");
    expect(envelope.error?.message).toContain("interval");
  });

  it("--once human validation error uses VALIDATION_ERROR code", async () => {
    const repo = await makeRepo();
    await initRepo(repo);

    const result = await runCli(repo, ["watch", "--once", "--interval", "abc"]);
    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("VALIDATION_ERROR");
  });
});
