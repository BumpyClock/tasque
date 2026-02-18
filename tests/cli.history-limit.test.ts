import { afterEach, describe, expect, it } from "bun:test";
import { cleanupRepos, makeRepo as makeRepoBase, okData, runJson as runJsonBase } from "./helpers";

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-history-limit-");
}

afterEach(cleanupRepos);

async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "test-limit");
}

describe("cli history --limit validation", () => {
  it("rejects --limit foo with validation error", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Limit test"])).envelope,
    ).task;

    const result = await runJson(repo, ["history", created.id, "--limit", "foo"]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("rejects --limit 0 with validation error", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Limit zero test"])).envelope,
    ).task;

    const result = await runJson(repo, ["history", created.id, "--limit", "0"]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("rejects --limit -1 with validation error", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Limit negative test"])).envelope,
    ).task;

    const result = await runJson(repo, ["history", created.id, "--limit", "-1"]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("accepts --limit 1 and returns results", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Limit valid test"])).envelope,
    ).task;

    await runJson(repo, ["update", created.id, "--title", "Updated"]);

    const result = await runJson(repo, ["history", created.id, "--limit", "1"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ events: Array<Record<string, unknown>>; count: number }>(result.envelope);
    expect(data.events).toHaveLength(1);
  });

  it("accepts --limit 5 with positive value", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Limit five test"])).envelope,
    ).task;

    const result = await runJson(repo, ["history", created.id, "--limit", "5"]);
    expect(result.exitCode).toBe(0);
    expect(result.envelope.ok).toBe(true);
  });
});
