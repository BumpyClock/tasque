import { afterEach, describe, expect, it } from "bun:test";
import { cleanupRepos, makeRepo as makeRepoBase, okData, runJson as runJsonBase } from "./helpers";

async function makeRepo() {
  return makeRepoBase("tasque-stale-limit-");
}
afterEach(cleanupRepos);
async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "test-stale-limit");
}

describe("cli stale --limit", () => {
  it("limits results to specified count", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    // Create 5 tasks
    for (let i = 0; i < 5; i++) {
      await runJson(repo, ["create", `Stale task ${i}`]);
    }
    // All tasks are stale at --days 0
    const unlimited = await runJson(repo, ["stale", "--days", "0"]);
    expect(unlimited.exitCode).toBe(0);
    const allTasks = okData<{ tasks: Array<{ id: string }> }>(unlimited.envelope).tasks;
    expect(allTasks.length).toBe(5);

    // Limit to 2
    const limited = await runJson(repo, ["stale", "--days", "0", "--limit", "2"]);
    expect(limited.exitCode).toBe(0);
    const limitedTasks = okData<{ tasks: Array<{ id: string }> }>(limited.envelope).tasks;
    expect(limitedTasks.length).toBe(2);
  });

  it("preserves sort order when limited", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    await runJson(repo, ["create", "First task"]);
    await runJson(repo, ["create", "Second task"]);
    await runJson(repo, ["create", "Third task"]);

    const unlimited = await runJson(repo, ["stale", "--days", "0"]);
    const allTasks = okData<{ tasks: Array<{ id: string }> }>(unlimited.envelope).tasks;

    const limited = await runJson(repo, ["stale", "--days", "0", "--limit", "2"]);
    const limitedTasks = okData<{ tasks: Array<{ id: string }> }>(limited.envelope).tasks;

    // Limited should be first 2 of unlimited (same order)
    expect(limitedTasks[0]?.id).toBe(allTasks[0]?.id);
    expect(limitedTasks[1]?.id).toBe(allTasks[1]?.id);
  });

  it("rejects zero limit", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const result = await runJson(repo, ["stale", "--days", "0", "--limit", "0"]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
  });

  it("rejects negative limit", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const result = await runJson(repo, ["stale", "--days", "0", "--limit", "-1"]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
  });

  it("limit larger than result count returns all", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    await runJson(repo, ["create", "Single task"]);
    const result = await runJson(repo, ["stale", "--days", "0", "--limit", "100"]);
    expect(result.exitCode).toBe(0);
    const tasks = okData<{ tasks: Array<{ id: string }> }>(result.envelope).tasks;
    expect(tasks.length).toBe(1);
  });
});
