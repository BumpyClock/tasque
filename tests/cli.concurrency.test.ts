import { afterEach, describe, expect, it } from "bun:test";
import { cleanupRepos, makeRepo as makeRepoBase, okData, runJson } from "./helpers";

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-cli-concurrency-");
}

afterEach(cleanupRepos);

describe("cli concurrency", () => {
  it("allows exactly one winner in 5 concurrent claims", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"], "setup");

    const created = await runJson(repo, ["create", "Claim target"], "setup");
    const taskId = okData<{ task: { id: string } }>(created.envelope).task.id;

    const claimAttempts = await Promise.all(
      Array.from({ length: 5 }, (_, idx) =>
        runJson(repo, ["update", taskId, "--claim", "--assignee", `agent-${idx}`], `runner-${idx}`),
      ),
    );

    const winners = claimAttempts.filter((result) => result.envelope.ok);
    const losers = claimAttempts.filter((result) => !result.envelope.ok);

    expect(winners.length).toBe(1);
    expect(losers.length).toBe(4);

    for (const loser of losers) {
      expect(loser.exitCode).toBe(1);
      expect(loser.envelope.error?.code).toBe("CLAIM_CONFLICT");
    }

    const shown = await runJson(repo, ["show", taskId], "verify");
    expect(shown.exitCode).toBe(0);
    const shownTask = okData<{ task: { assignee?: string; status: string } }>(shown.envelope).task;
    expect(typeof shownTask.assignee).toBe("string");
    expect((shownTask.assignee ?? "").startsWith("agent-")).toBe(true);
    expect(shownTask.status).toBe("in_progress");
  });

  it("two concurrent dep add commands for different blockers do not corrupt state", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"], "setup");

    const childResult = await runJson(repo, ["create", "Child task"], "setup");
    const childId = okData<{ task: { id: string } }>(childResult.envelope).task.id;

    const blockerAResult = await runJson(repo, ["create", "Blocker A"], "setup");
    const blockerAId = okData<{ task: { id: string } }>(blockerAResult.envelope).task.id;

    const blockerBResult = await runJson(repo, ["create", "Blocker B"], "setup");
    const blockerBId = okData<{ task: { id: string } }>(blockerBResult.envelope).task.id;

    const results = await Promise.all([
      runJson(repo, ["dep", "add", childId, blockerAId], "agent-0"),
      runJson(repo, ["dep", "add", childId, blockerBId], "agent-1"),
    ]);

    for (const result of results) {
      expect(result.envelope.ok).toBe(true);
      expect(result.exitCode).toBe(0);
    }

    const shown = await runJson(repo, ["show", childId], "verify");
    expect(shown.exitCode).toBe(0);
    const showData = okData<{ blockers: string[] }>(shown.envelope);
    expect(showData.blockers).toContain(blockerAId);
    expect(showData.blockers).toContain(blockerBId);
    expect(showData.blockers).toHaveLength(2);
  });

  it("two concurrent dep add commands for the same blocker both succeed without duplication", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"], "setup");

    const childResult = await runJson(repo, ["create", "Child task"], "setup");
    const childId = okData<{ task: { id: string } }>(childResult.envelope).task.id;

    const blockerResult = await runJson(repo, ["create", "Shared blocker"], "setup");
    const blockerId = okData<{ task: { id: string } }>(blockerResult.envelope).task.id;

    const results = await Promise.all([
      runJson(repo, ["dep", "add", childId, blockerId], "agent-0"),
      runJson(repo, ["dep", "add", childId, blockerId], "agent-1"),
    ]);

    for (const result of results) {
      expect(result.envelope.ok).toBe(true);
      expect(result.exitCode).toBe(0);
    }

    const shown = await runJson(repo, ["show", childId], "verify");
    expect(shown.exitCode).toBe(0);
    const showData = okData<{ blockers: string[] }>(shown.envelope);
    expect(showData.blockers).toEqual([blockerId]);
  });
});
