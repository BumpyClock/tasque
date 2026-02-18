import { afterEach, describe, expect, it } from "bun:test";
import { cleanupRepos, makeRepo as makeRepoBase, okData, runJson as runJsonBase } from "./helpers";

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-discovered-from-");
}

afterEach(cleanupRepos);

async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "discovered-from-test");
}

describe("cli discovered_from", () => {
  it("supports create/update/clear/show/list/search flow", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const sourceA = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Source A"])).envelope,
    ).task;
    const sourceB = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Source B"])).envelope,
    ).task;

    const derived = okData<{ task: { id: string; discovered_from?: string } }>(
      (await runJson(repo, ["create", "Derived", "--discovered-from", sourceA.id])).envelope,
    ).task;
    expect(derived.discovered_from).toBe(sourceA.id);

    const shown = okData<{ task: { discovered_from?: string } }>(
      (await runJson(repo, ["show", derived.id])).envelope,
    );
    expect(shown.task.discovered_from).toBe(sourceA.id);

    const listed = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["list", "--discovered-from", sourceA.id])).envelope,
    ).tasks;
    expect(listed.map((task) => task.id)).toEqual([derived.id]);

    const searched = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["search", `discovered_from:${sourceA.id}`])).envelope,
    ).tasks;
    expect(searched.map((task) => task.id)).toEqual([derived.id]);

    const updated = okData<{ task: { discovered_from?: string } }>(
      (await runJson(repo, ["update", derived.id, "--discovered-from", sourceB.id])).envelope,
    ).task;
    expect(updated.discovered_from).toBe(sourceB.id);

    const cleared = okData<{ task: { discovered_from?: string } }>(
      (await runJson(repo, ["update", derived.id, "--clear-discovered-from"])).envelope,
    ).task;
    expect(cleared.discovered_from).toBeUndefined();
  });

  it("rejects unknown discovered-from references", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const createBad = await runJson(repo, [
      "create",
      "Broken discovered reference",
      "--discovered-from",
      "tsq-missing",
    ]);
    expect(createBad.exitCode).toBe(1);
    expect(createBad.envelope.ok).toBe(false);

    const base = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Base task"])).envelope,
    ).task;
    const updateBad = await runJson(repo, ["update", base.id, "--discovered-from", "tsq-missing"]);
    expect(updateBad.exitCode).toBe(1);
    expect(updateBad.envelope.ok).toBe(false);
  });

  it("rejects conflicting discovered-from update flags", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const source = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Source"])).envelope,
    ).task;
    const task = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Task"])).envelope,
    ).task;

    const bad = await runJson(repo, [
      "update",
      task.id,
      "--discovered-from",
      source.id,
      "--clear-discovered-from",
    ]);
    expect(bad.exitCode).toBe(1);
    expect(bad.envelope.ok).toBe(false);
    expect(bad.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("does not affect ready semantics", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const source = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Source"])).envelope,
    ).task;
    const derived = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Derived", "--discovered-from", source.id])).envelope,
    ).task;

    const ready = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["ready"])).envelope,
    ).tasks;
    const readyIds = ready.map((task) => task.id);
    expect(readyIds).toContain(source.id);
    expect(readyIds).toContain(derived.id);
  });
});
