import { afterEach, describe, expect, it } from "bun:test";
import type { Task } from "../src/types";
import { cleanupRepos, makeRepo as makeRepoBase, okData, runJson as runJsonBase } from "./helpers";

async function makeRepo() {
  return makeRepoBase("tasque-ready-lane-");
}
afterEach(cleanupRepos);
async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "test-lane");
}

describe("ready --lane", () => {
  it("ready --lane planning returns only tasks with planning_state=needs_planning", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    await runJson(repo, ["create", "Needs planning", "--planning", "needs_planning"]);
    await runJson(repo, ["create", "Already planned", "--planning", "planned"]);

    const r = await runJson(repo, ["ready", "--lane", "planning"]);
    const tasks = okData<{ tasks: Task[] }>(r.envelope).tasks;
    expect(tasks.length).toBe(1);
    expect(tasks[0]?.title).toBe("Needs planning");
  });

  it("ready --lane coding returns only tasks with planning_state=planned", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    await runJson(repo, ["create", "Needs planning", "--planning", "needs_planning"]);
    await runJson(repo, ["create", "Already planned", "--planning", "planned"]);

    const r = await runJson(repo, ["ready", "--lane", "coding"]);
    const tasks = okData<{ tasks: Task[] }>(r.envelope).tasks;
    expect(tasks.length).toBe(1);
    expect(tasks[0]?.title).toBe("Already planned");
  });

  it("ready (no lane) returns all ready tasks regardless of planning_state", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    await runJson(repo, ["create", "Needs planning", "--planning", "needs_planning"]);
    await runJson(repo, ["create", "Already planned", "--planning", "planned"]);

    const r = await runJson(repo, ["ready"]);
    const tasks = okData<{ tasks: Task[] }>(r.envelope).tasks;
    expect(tasks.length).toBe(2);
  });

  it("ready --lane with invalid value returns validation error", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const r = await runJson(repo, ["ready", "--lane", "invalid"]);
    expect(r.envelope.ok).toBe(false);
    expect(r.envelope.error?.code).toBe("VALIDATION_ERROR");
  });
});
