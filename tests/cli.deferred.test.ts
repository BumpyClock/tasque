import { afterEach, describe, expect, it } from "bun:test";
import type { Task } from "../src/types";
import { cleanupRepos, makeRepo as makeRepoBase, okData, runJson as runJsonBase } from "./helpers";

async function makeRepo() {
  return makeRepoBase("tasque-deferred-");
}
afterEach(cleanupRepos);
async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "test-deferred");
}

describe("deferred status", () => {
  it("create + update to deferred status works", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const r = await runJson(repo, ["create", "Defer me"]);
    const task = okData<{ task: Task }>(r.envelope).task;

    const u = await runJson(repo, ["update", task.id, "--status", "deferred"]);
    expect(u.envelope.ok).toBe(true);
    const updated = okData<{ task: Task }>(u.envelope).task;
    expect(updated.status).toBe("deferred");
  });

  it("deferred tasks appear in stale output", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const r = await runJson(repo, ["create", "Stale deferred"]);
    const task = okData<{ task: Task }>(r.envelope).task;
    await runJson(repo, ["update", task.id, "--status", "deferred"]);

    // With days=0, everything updated at or before now is stale
    const stale = await runJson(repo, ["stale", "--days", "0"]);
    const result = okData<{ tasks: Task[] }>(stale.envelope);
    const found = result.tasks.some((t) => t.id === task.id);
    expect(found).toBe(true);
  });

  it("deferred tasks are NOT ready", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const r = await runJson(repo, ["create", "Not ready when deferred"]);
    const task = okData<{ task: Task }>(r.envelope).task;
    await runJson(repo, ["update", task.id, "--status", "deferred"]);

    const ready = await runJson(repo, ["ready"]);
    const tasks = okData<{ tasks: Task[] }>(ready.envelope).tasks;
    const found = tasks.some((t) => t.id === task.id);
    expect(found).toBe(false);
  });

  it("deferred tasks can be listed with --status deferred", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const r = await runJson(repo, ["create", "List deferred"]);
    const task = okData<{ task: Task }>(r.envelope).task;
    await runJson(repo, ["update", task.id, "--status", "deferred"]);

    const list = await runJson(repo, ["list", "--status", "deferred"]);
    const tasks = okData<{ tasks: Task[] }>(list.envelope).tasks;
    expect(tasks.length).toBe(1);
    const first = tasks[0];
    expect(first).toBeDefined();
    expect(first?.id).toBe(task.id);
    expect(first?.status).toBe("deferred");
  });
});
