import { afterEach, describe, expect, it } from "bun:test";
import {
  assertEnvelopeShape,
  cleanupRepos,
  makeRepo as makeRepoBase,
  okData,
  runCli as runCliBase,
  runJson as runJsonBase,
} from "./helpers";

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-labels-e2e-");
}

afterEach(cleanupRepos);

async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "test-labels");
}

async function runCli(repoDir: string, args: string[]) {
  return runCliBase(repoDir, args, "test-labels");
}

describe("cli label commands", () => {
  it("label add adds label to task", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Label add target"])).envelope,
    ).task;

    const result = await runJson(repo, ["label", "add", created.id, "bug"]);
    expect(result.exitCode).toBe(0);
    expect(result.envelope.ok).toBe(true);

    const shown = okData<{ task: { id: string; labels: string[] } }>(
      (await runJson(repo, ["show", created.id])).envelope,
    );
    expect(shown.task.labels).toContain("bug");
  });

  it("label add normalizes label to lowercase", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Label normalize target"])).envelope,
    ).task;

    const result = await runJson(repo, ["label", "add", created.id, "BUG"]);
    expect(result.exitCode).toBe(0);

    const task = okData<{ task: { labels: string[] } }>(result.envelope).task;
    expect(task.labels).toContain("bug");
    expect(task.labels).not.toContain("BUG");
  });

  it("label add is idempotent when label already exists", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Label idempotent target"])).envelope,
    ).task;

    await runJson(repo, ["label", "add", created.id, "feature"]);
    const second = await runJson(repo, ["label", "add", created.id, "feature"]);
    expect(second.exitCode).toBe(0);

    const task = okData<{ task: { labels: string[] } }>(second.envelope).task;
    const featureCount = task.labels.filter((label) => label === "feature").length;
    expect(featureCount).toBe(1);
  });

  it("label add rejects label with invalid characters", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Label invalid chars target"])).envelope,
    ).task;

    const result = await runJson(repo, ["label", "add", created.id, "invalid label"]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("label remove removes label from task", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Label remove target"])).envelope,
    ).task;

    await runJson(repo, ["label", "add", created.id, "to-remove"]);
    const result = await runJson(repo, ["label", "remove", created.id, "to-remove"]);
    expect(result.exitCode).toBe(0);
    expect(result.envelope.ok).toBe(true);

    const task = okData<{ task: { labels: string[] } }>(result.envelope).task;
    expect(task.labels).not.toContain("to-remove");
  });

  it("label remove rejects removing a label that does not exist on task", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Label remove missing target"])).envelope,
    ).task;

    const result = await runJson(repo, ["label", "remove", created.id, "nonexistent"]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("NOT_FOUND");
  });

  it("label list aggregates labels with counts across tasks", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const taskA = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Label list task A"])).envelope,
    ).task;
    const taskB = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Label list task B"])).envelope,
    ).task;
    const taskC = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Label list task C"])).envelope,
    ).task;

    await runJson(repo, ["label", "add", taskA.id, "bug"]);
    await runJson(repo, ["label", "add", taskB.id, "bug"]);
    await runJson(repo, ["label", "add", taskC.id, "feature"]);

    const result = await runJson(repo, ["label", "list"]);
    expect(result.exitCode).toBe(0);
    expect(result.envelope.ok).toBe(true);

    const data = okData<{ labels: Array<{ label: string; count: number }> }>(result.envelope);
    expect(Array.isArray(data.labels)).toBe(true);

    const bugEntry = data.labels.find((entry) => entry.label === "bug");
    const featureEntry = data.labels.find((entry) => entry.label === "feature");
    expect(bugEntry).toBeDefined();
    expect(bugEntry?.count).toBe(2);
    expect(featureEntry).toBeDefined();
    expect(featureEntry?.count).toBe(1);
  });

  it("list --label filters tasks to only those with the matching label", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const taggedTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Tagged task"])).envelope,
    ).task;
    const untaggedTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Untagged task"])).envelope,
    ).task;

    await runJson(repo, ["label", "add", taggedTask.id, "priority-high"]);

    const result = await runJson(repo, ["list", "--label", "priority-high"]);
    expect(result.exitCode).toBe(0);
    expect(result.envelope.ok).toBe(true);

    const data = okData<{ tasks: Array<{ id: string }> }>(result.envelope);
    const ids = data.tasks.map((task) => task.id);
    expect(ids).toContain(taggedTask.id);
    expect(ids).not.toContain(untaggedTask.id);
  });

  it("label add and remove return task JSON envelope with correct shape", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "JSON envelope target"])).envelope,
    ).task;

    const addResult = await runJson(repo, ["label", "add", created.id, "verified"]);
    expect(addResult.exitCode).toBe(0);
    assertEnvelopeShape(addResult.envelope);
    const addData = okData<{ task: Record<string, unknown> }>(addResult.envelope);
    expect(typeof addData.task).toBe("object");
    expect(addData.task.id).toBe(created.id);
    expect(Array.isArray(addData.task.labels)).toBe(true);

    const removeResult = await runJson(repo, ["label", "remove", created.id, "verified"]);
    expect(removeResult.exitCode).toBe(0);
    assertEnvelopeShape(removeResult.envelope);
    const removeData = okData<{ task: Record<string, unknown> }>(removeResult.envelope);
    expect(typeof removeData.task).toBe("object");
    expect(removeData.task.id).toBe(created.id);
    expect(Array.isArray(removeData.task.labels)).toBe(true);

    const listResult = await runJson(repo, ["label", "list"]);
    expect(listResult.exitCode).toBe(0);
    assertEnvelopeShape(listResult.envelope);
    const listData = okData<{ labels: unknown }>(listResult.envelope);
    expect(Array.isArray(listData.labels)).toBe(true);
  });

  it("label add rejects empty label string", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Label empty string target"])).envelope,
    ).task;

    const result = await runCli(repo, ["label", "add", created.id, ""]);
    expect(result.exitCode).toBe(1);
  });

  it("label add rejects label exceeding 64 characters", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Label long string target"])).envelope,
    ).task;

    const longLabel = "a".repeat(65);
    const result = await runJson(repo, ["label", "add", created.id, longLabel]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });
});
