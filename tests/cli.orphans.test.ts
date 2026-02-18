import { afterEach, describe, expect, it } from "bun:test";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { cleanupRepos, makeRepo as makeRepoBase, okData, runJson as runJsonBase } from "./helpers";

async function makeRepo() {
  return makeRepoBase("tasque-orphans-");
}
afterEach(cleanupRepos);
async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "test-orphans");
}

// Helper to inject orphan dep into state.json (bypasses projector validation)
async function injectOrphanDep(repoDir: string, child: string, blocker: string): Promise<void> {
  const stateFile = join(repoDir, ".tasque", "state.json");
  const raw = await readFile(stateFile, "utf8");
  const state = JSON.parse(raw);
  const deps: string[] = state.deps[child] ?? [];
  if (!deps.includes(blocker)) {
    deps.push(blocker);
  }
  state.deps[child] = deps;
  await writeFile(stateFile, JSON.stringify(state, null, 2), "utf8");
}

async function injectOrphanLink(
  repoDir: string,
  src: string,
  dst: string,
  type: string,
): Promise<void> {
  const stateFile = join(repoDir, ".tasque", "state.json");
  const raw = await readFile(stateFile, "utf8");
  const state = JSON.parse(raw);
  if (!state.links[src]) state.links[src] = {};
  const existing: string[] = state.links[src][type] ?? [];
  if (!existing.includes(dst)) existing.push(dst);
  state.links[src][type] = existing;
  await writeFile(stateFile, JSON.stringify(state, null, 2), "utf8");
}

describe("cli orphans", () => {
  it("reports clean when no orphans", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    await runJson(repo, ["create", "Task A"]);

    const result = await runJson(repo, ["orphans"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ orphaned_deps: unknown[]; orphaned_links: unknown[]; total: number }>(
      result.envelope,
    );
    expect(data.total).toBe(0);
    expect(data.orphaned_deps).toEqual([]);
    expect(data.orphaned_links).toEqual([]);
  });

  it("detects orphaned deps", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const task = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Real task"])).envelope,
    ).task;

    await injectOrphanDep(repo, task.id, "tsq-nonexist");

    const result = await runJson(repo, ["orphans"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{
      orphaned_deps: Array<{ child: string; blocker: string }>;
      total: number;
    }>(result.envelope);
    expect(data.total).toBeGreaterThan(0);
    expect(data.orphaned_deps.some((d) => d.blocker === "tsq-nonexist")).toBe(true);
  });

  it("detects orphaned links", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const task = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Real task"])).envelope,
    ).task;

    await injectOrphanLink(repo, task.id, "tsq-ghost", "relates_to");

    const result = await runJson(repo, ["orphans"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{
      orphaned_links: Array<{ src: string; dst: string; type: string }>;
      total: number;
    }>(result.envelope);
    expect(data.total).toBeGreaterThan(0);
    expect(data.orphaned_links.some((l) => l.dst === "tsq-ghost")).toBe(true);
  });

  it("is read-only (does not append events)", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const task = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Task"])).envelope,
    ).task;
    await injectOrphanDep(repo, task.id, "tsq-nonexist");

    // Get event count before
    const eventsBefore = (await readFile(join(repo, ".tasque", "events.jsonl"), "utf8"))
      .trim()
      .split("\n").length;

    await runJson(repo, ["orphans"]);

    // Event count should not change
    const eventsAfter = (await readFile(join(repo, ".tasque", "events.jsonl"), "utf8"))
      .trim()
      .split("\n").length;
    expect(eventsAfter).toBe(eventsBefore);
  });

  it("json envelope has correct command name", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const result = await runJson(repo, ["orphans"]);
    expect(result.envelope.command).toBe("tsq orphans");
  });
});
