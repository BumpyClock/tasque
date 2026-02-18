import { afterEach, describe, expect, it } from "bun:test";
import { cleanupRepos, makeRepo as makeRepoBase, okData, runJson as runJsonBase } from "./helpers";

async function makeRepo() {
  return makeRepoBase("tasque-planning-proj-");
}
afterEach(cleanupRepos);
async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "test-planning");
}

describe("projector planning_state", () => {
  it("planning_state persists through task.created event replay", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const r = await runJson(repo, ["create", "Plan me", "--planning", "planned"]);
    const task = okData<{ task: { id: string; planning_state: string } }>(r.envelope).task;
    expect(task.planning_state).toBe("planned");

    // Verify via show
    const show = await runJson(repo, ["show", task.id]);
    const shown = okData<{ task: { planning_state: string } }>(show.envelope).task;
    expect(shown.planning_state).toBe("planned");
  });

  it("planning_state defaults to needs_planning when absent from payload", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const r = await runJson(repo, ["create", "No planning flag"]);
    const task = okData<{ task: { planning_state: string } }>(r.envelope).task;
    expect(task.planning_state).toBe("needs_planning");
  });

  it("planning_state can be updated via task.updated event", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const r = await runJson(repo, ["create", "Update planning"]);
    const task = okData<{ task: { id: string; planning_state: string } }>(r.envelope).task;
    expect(task.planning_state).toBe("needs_planning");

    const u = await runJson(repo, ["update", task.id, "--planning", "planned"]);
    const updated = okData<{ task: { planning_state: string } }>(u.envelope).task;
    expect(updated.planning_state).toBe("planned");
  });

  it("legacy events without planning_state replay correctly", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    // Create a task — it will have planning_state=needs_planning by default
    const r = await runJson(repo, ["create", "Legacy compat"]);
    const task = okData<{ task: { id: string; planning_state: string } }>(r.envelope).task;
    // The task should still work fine; planning_state is set by the projector default
    expect(task.planning_state).toBe("needs_planning");

    // Listing should not crash
    const list = await runJson(repo, ["list"]);
    expect(list.envelope.ok).toBe(true);
  });
});
