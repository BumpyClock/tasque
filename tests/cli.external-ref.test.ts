import { afterEach, describe, expect, it } from "bun:test";
import { cleanupRepos, makeRepo as makeRepoBase, okData, runJson as runJsonBase } from "./helpers";

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-extref-");
}

afterEach(cleanupRepos);

async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "extref-test");
}

describe("cli external_ref", () => {
  it("supports create/update/show roundtrip with clear path", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string; external_ref?: string } }>(
      (await runJson(repo, ["create", "Ext ref task", "--external-ref", "GH-101"])).envelope,
    ).task;
    expect(created.external_ref).toBe("GH-101");

    const updated = okData<{ task: { external_ref?: string } }>(
      (await runJson(repo, ["update", created.id, "--external-ref", "JIRA-42"])).envelope,
    ).task;
    expect(updated.external_ref).toBe("JIRA-42");

    const shown = okData<{ task: { external_ref?: string } }>(
      (await runJson(repo, ["show", created.id])).envelope,
    ).task;
    expect(shown.external_ref).toBe("JIRA-42");

    const cleared = okData<{ task: { external_ref?: string } }>(
      (await runJson(repo, ["update", created.id, "--clear-external-ref"])).envelope,
    ).task;
    expect(cleared.external_ref).toBeUndefined();
  });

  it("filters list by --external-ref exact match", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const first = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "First", "--external-ref", "GH-123"])).envelope,
    ).task;
    await runJson(repo, ["create", "Second", "--external-ref", "GH-999"]);

    const listed = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["list", "--external-ref", "GH-123"])).envelope,
    ).tasks;
    expect(listed.map((task) => task.id)).toEqual([first.id]);
  });

  it("supports external_ref fielded search", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const matched = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Search ext ref", "--external-ref", "ENG-2201"])).envelope,
    ).task;
    await runJson(repo, ["create", "Search no ref"]);

    const result = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["search", "external_ref:ENG-2201"])).envelope,
    ).tasks;
    expect(result.map((task) => task.id)).toEqual([matched.id]);
  });

  it("rejects invalid update flag combinations with external_ref", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Combo"])).envelope,
    ).task;

    const badUpdate = await runJson(repo, [
      "update",
      created.id,
      "--external-ref",
      "X-1",
      "--clear-external-ref",
    ]);
    expect(badUpdate.exitCode).toBe(1);
    expect(badUpdate.envelope.ok).toBe(false);
    expect(badUpdate.envelope.error?.code).toBe("VALIDATION_ERROR");

    const badClaim = await runJson(repo, [
      "update",
      created.id,
      "--claim",
      "--external-ref",
      "X-2",
    ]);
    expect(badClaim.exitCode).toBe(1);
    expect(badClaim.envelope.ok).toBe(false);
    expect(badClaim.envelope.error?.code).toBe("VALIDATION_ERROR");
  });
});
