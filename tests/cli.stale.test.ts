import { afterEach, describe, expect, it } from "bun:test";
import { cleanupRepos, makeRepo as makeRepoBase, okData, runJson as runJsonBase } from "./helpers";

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-stale-e2e-");
}

afterEach(cleanupRepos);

async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "test-stale");
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
