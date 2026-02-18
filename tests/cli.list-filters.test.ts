import { afterEach, describe, expect, it } from "bun:test";
import { type JsonEnvelope, cleanupRepos, makeRepo as makeRepoBase, okData, runJson as runJsonBase } from "./helpers";

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-list-filters-e2e-");
}

afterEach(cleanupRepos);

async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "test-list-filters");
}

function listTaskIds(envelope: JsonEnvelope): string[] {
  return okData<{ tasks: Array<{ id: string }> }>(envelope).tasks.map((task) => task.id);
}

describe("cli list filters", () => {
  it("supports --label-any csv/repeat with --id and --unassigned", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const alpha = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Alpha list filter target"])).envelope,
    ).task;
    const beta = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Beta list filter target"])).envelope,
    ).task;
    const gamma = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Gamma list filter target"])).envelope,
    ).task;

    await runJson(repo, ["label", "add", alpha.id, "bug"]);
    await runJson(repo, ["label", "add", beta.id, "docs"]);
    await runJson(repo, ["label", "add", gamma.id, "ui"]);
    await runJson(repo, ["update", beta.id, "--claim", "--assignee", "bob"]);

    const listed = await runJson(repo, [
      "list",
      "--label-any",
      "bug,ui",
      "--label-any",
      "ops",
      "--unassigned",
      "--id",
      `${alpha.id},${beta.id}`,
      "--id",
      `${gamma.id},${alpha.id}`,
    ]);

    expect(listed.exitCode).toBe(0);
    expect(listed.envelope.command).toBe("tsq list");
    expect(listTaskIds(listed.envelope)).toEqual([alpha.id, gamma.id]);
  });

  it("applies created/updated/closed after as strict ISO comparisons", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const createdTask = okData<{ task: { id: string; created_at: string; updated_at: string } }>(
      (await runJson(repo, ["create", "Date filter target"])).envelope,
    ).task;

    const createdAfterEqual = await runJson(repo, [
      "list",
      "--created-after",
      createdTask.created_at,
    ]);
    expect(createdAfterEqual.exitCode).toBe(0);
    expect(listTaskIds(createdAfterEqual.envelope)).toEqual([]);

    await Bun.sleep(10);
    const updatedTask = okData<{ task: { updated_at: string } }>(
      (await runJson(repo, ["update", createdTask.id, "--status", "in_progress"])).envelope,
    ).task;

    const updatedAfterCreated = await runJson(repo, [
      "list",
      "--updated-after",
      createdTask.updated_at,
    ]);
    expect(updatedAfterCreated.exitCode).toBe(0);
    expect(listTaskIds(updatedAfterCreated.envelope)).toEqual([createdTask.id]);

    const updatedAfterEqual = await runJson(repo, [
      "list",
      "--updated-after",
      updatedTask.updated_at,
    ]);
    expect(updatedAfterEqual.exitCode).toBe(0);
    expect(listTaskIds(updatedAfterEqual.envelope)).toEqual([]);

    await Bun.sleep(10);
    await runJson(repo, ["close", createdTask.id]);
    const closedTask = okData<{ task: { closed_at?: string } }>(
      (await runJson(repo, ["show", createdTask.id])).envelope,
    ).task;
    expect(typeof closedTask.closed_at).toBe("string");

    const closedAfterUpdate = await runJson(repo, [
      "list",
      "--closed-after",
      updatedTask.updated_at,
      "--status",
      "closed",
    ]);
    expect(closedAfterUpdate.exitCode).toBe(0);
    expect(listTaskIds(closedAfterUpdate.envelope)).toEqual([createdTask.id]);

    const closedAfterEqual = await runJson(repo, [
      "list",
      "--closed-after",
      String(closedTask.closed_at),
      "--status",
      "closed",
    ]);
    expect(closedAfterEqual.exitCode).toBe(0);
    expect(listTaskIds(closedAfterEqual.envelope)).toEqual([]);
  });

  it("returns validation errors for invalid filter combinations and values", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const assigneeConflict = await runJson(repo, ["list", "--assignee", "alice", "--unassigned"]);
    expect(assigneeConflict.exitCode).toBe(1);
    expect(assigneeConflict.envelope.ok).toBe(false);
    expect(assigneeConflict.envelope.error?.code).toBe("VALIDATION_ERROR");

    const invalidCreatedAfter = await runJson(repo, ["list", "--created-after", "not-an-iso"]);
    expect(invalidCreatedAfter.exitCode).toBe(1);
    expect(invalidCreatedAfter.envelope.ok).toBe(false);
    expect(invalidCreatedAfter.envelope.error?.code).toBe("VALIDATION_ERROR");

    const invalidLabelAny = await runJson(repo, ["list", "--label-any", "bug,"]);
    expect(invalidLabelAny.exitCode).toBe(1);
    expect(invalidLabelAny.envelope.ok).toBe(false);
    expect(invalidLabelAny.envelope.error?.code).toBe("VALIDATION_ERROR");

    const invalidIdList = await runJson(repo, ["list", "--id", ","]);
    expect(invalidIdList.exitCode).toBe(1);
    expect(invalidIdList.envelope.ok).toBe(false);
    expect(invalidIdList.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("returns validation error for --assignee=<value> with --unassigned", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const assigneeEqualsConflict = await runJson(repo, [
      "list",
      "--assignee=alice",
      "--unassigned",
    ]);
    expect(assigneeEqualsConflict.exitCode).toBe(1);
    expect(assigneeEqualsConflict.envelope.ok).toBe(false);
    expect(assigneeEqualsConflict.envelope.error?.code).toBe("VALIDATION_ERROR");

    const assigneeEmptyConflict = await runJson(repo, ["list", "--assignee=", "--unassigned"]);
    expect(assigneeEmptyConflict.exitCode).toBe(1);
    expect(assigneeEmptyConflict.envelope.ok).toBe(false);
    expect(assigneeEmptyConflict.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("returns validation error for natural-language --created-after values", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const naturalLanguageCreatedAfter = await runJson(repo, [
      "list",
      "--created-after",
      "March 1, 2024",
    ]);
    expect(naturalLanguageCreatedAfter.exitCode).toBe(1);
    expect(naturalLanguageCreatedAfter.envelope.ok).toBe(false);
    expect(naturalLanguageCreatedAfter.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("keeps list json output deterministic for equivalent repeatable csv inputs", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const first = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Deterministic A"])).envelope,
    ).task;
    const second = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Deterministic B"])).envelope,
    ).task;

    await runJson(repo, ["label", "add", first.id, "ops"]);
    await runJson(repo, ["label", "add", second.id, "ops"]);

    const baseline = await runJson(repo, [
      "list",
      "--id",
      `${first.id},${second.id}`,
      "--label-any",
      "ops",
    ]);
    const equivalent = await runJson(repo, [
      "list",
      "--id",
      second.id,
      "--id",
      first.id,
      "--label-any",
      "ops,ops",
    ]);

    expect(baseline.exitCode).toBe(0);
    expect(equivalent.exitCode).toBe(0);
    expect(baseline.stdout).toBe(equivalent.stdout);
  });
});
