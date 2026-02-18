import { afterEach, describe, expect, it } from "bun:test";
import type { Task } from "../src/types";
import { type JsonEnvelope, assertEnvelopeShape, cleanupRepos, cliEntry, makeRepo as makeRepoBase, okData, runJson as runJsonBase } from "./helpers";

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-search-e2e-");
}

afterEach(cleanupRepos);

async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "test-search");
}

describe("cli search", () => {
  it("search by title substring returns only matching tasks", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const login = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Fix login bug"])).envelope,
    ).task;
    const feature = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Add feature"])).envelope,
    ).task;

    const result = await runJson(repo, ["search", "login"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ tasks: Array<{ id: string }> }>(result.envelope);
    const ids = data.tasks.map((task) => task.id);
    expect(ids.includes(login.id)).toBe(true);
    expect(ids.includes(feature.id)).toBe(false);
  });

  it("search by status returns only tasks with that status", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const openTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Status open task"])).envelope,
    ).task;
    const closedTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Status closed task"])).envelope,
    ).task;

    await runJson(repo, ["update", closedTask.id, "--status", "closed"]);

    const result = await runJson(repo, ["search", "status:open"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ tasks: Array<{ id: string }> }>(result.envelope);
    const ids = data.tasks.map((task) => task.id);
    expect(ids.includes(openTask.id)).toBe(true);
    expect(ids.includes(closedTask.id)).toBe(false);
  });

  it("search by label returns only tasks with that label", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const bugTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Label bug task"])).envelope,
    ).task;
    const otherTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Label other task"])).envelope,
    ).task;

    await runJson(repo, ["label", "add", bugTask.id, "bug"]);
    await runJson(repo, ["label", "add", otherTask.id, "feature"]);

    const result = await runJson(repo, ["search", "label:bug"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ tasks: Array<{ id: string }> }>(result.envelope);
    const ids = data.tasks.map((task) => task.id);
    expect(ids.includes(bugTask.id)).toBe(true);
    expect(ids.includes(otherTask.id)).toBe(false);
  });

  it("search by assignee returns only tasks claimed by that assignee", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const aliceTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Assignee alice task"])).envelope,
    ).task;
    const bobTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Assignee bob task"])).envelope,
    ).task;

    await runJson(repo, ["update", aliceTask.id, "--claim", "--assignee", "alice"]);
    await runJson(repo, ["update", bobTask.id, "--claim", "--assignee", "bob"]);

    const result = await runJson(repo, ["search", "assignee:alice"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ tasks: Array<{ id: string }> }>(result.envelope);
    const ids = data.tasks.map((task) => task.id);
    expect(ids.includes(aliceTask.id)).toBe(true);
    expect(ids.includes(bobTask.id)).toBe(false);
  });

  it("search by priority returns only tasks with that priority value", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const p0Task = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Priority zero task", "-p", "0"])).envelope,
    ).task;
    const p2Task = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Priority two task", "-p", "2"])).envelope,
    ).task;

    const result = await runJson(repo, ["search", "priority:0"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ tasks: Array<{ id: string }> }>(result.envelope);
    const ids = data.tasks.map((task) => task.id);
    expect(ids.includes(p0Task.id)).toBe(true);
    expect(ids.includes(p2Task.id)).toBe(false);
  });

  it("search with negation excludes tasks matching the negated term", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const openTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Negation open task"])).envelope,
    ).task;
    const closedTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Negation closed task"])).envelope,
    ).task;

    await runJson(repo, ["update", closedTask.id, "--status", "closed"]);

    // Use --json before -- so commander parses it as global option
    const proc = Bun.spawn({
      cmd: ["bun", "run", cliEntry, "search", "--json", "--", "-status:closed"],
      cwd: repo,
      env: { ...process.env, TSQ_ACTOR: "test-search" },
      stdout: "pipe",
      stderr: "pipe",
    });
    const [exitCode, stdout, stderr] = await Promise.all([
      proc.exited,
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
    ]);
    const trimmed = stdout.trim();
    expect(trimmed.length > 0).toBe(true);
    assertEnvelopeShape(JSON.parse(trimmed));
    const result = { exitCode, stdout, stderr, envelope: JSON.parse(trimmed) as JsonEnvelope };
    expect(result.exitCode).toBe(0);
    const data = okData<{ tasks: Array<{ id: string }> }>(result.envelope);
    const ids = data.tasks.map((task) => task.id);
    expect(ids.includes(openTask.id)).toBe(true);
    expect(ids.includes(closedTask.id)).toBe(false);
  });

  it("search by kind returns only tasks of that kind", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const epicTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Kind epic task", "--kind", "epic"])).envelope,
    ).task;
    const regularTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Kind regular task", "--kind", "task"])).envelope,
    ).task;

    const result = await runJson(repo, ["search", "kind:epic"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ tasks: Array<{ id: string }> }>(result.envelope);
    const ids = data.tasks.map((task) => task.id);
    expect(ids.includes(epicTask.id)).toBe(true);
    expect(ids.includes(regularTask.id)).toBe(false);
  });

  it("search by ready true returns only unblocked open tasks", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const readyTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Ready task no deps"])).envelope,
    ).task;
    const blockedChild = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Blocked child task"])).envelope,
    ).task;
    const blocker = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Blocker task"])).envelope,
    ).task;

    await runJson(repo, ["dep", "add", blockedChild.id, blocker.id]);

    const result = await runJson(repo, ["search", "ready:true"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ tasks: Array<{ id: string }> }>(result.envelope);
    const ids = data.tasks.map((task) => task.id);
    expect(ids.includes(readyTask.id)).toBe(true);
    expect(ids.includes(blockedChild.id)).toBe(false);
  });

  it("search combines multiple terms with AND logic", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const bugOpenTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "And logic bug open task"])).envelope,
    ).task;
    const bugClosedTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "And logic bug closed task"])).envelope,
    ).task;
    const featureTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "And logic feature task"])).envelope,
    ).task;

    await runJson(repo, ["label", "add", bugOpenTask.id, "bug"]);
    await runJson(repo, ["label", "add", bugClosedTask.id, "bug"]);
    await runJson(repo, ["update", bugClosedTask.id, "--status", "closed"]);

    const result = await runJson(repo, ["search", "status:open label:bug"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ tasks: Array<{ id: string }> }>(result.envelope);
    const ids = data.tasks.map((task) => task.id);
    expect(ids.includes(bugOpenTask.id)).toBe(true);
    expect(ids.includes(bugClosedTask.id)).toBe(false);
    expect(ids.includes(featureTask.id)).toBe(false);
  });

  it("search with quoted title matches exact phrase in title", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const specificTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "my specific task title"])).envelope,
    ).task;
    const otherTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "unrelated task"])).envelope,
    ).task;

    const result = await runJson(repo, ["search", "my specific task"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ tasks: Array<{ id: string }> }>(result.envelope);
    const ids = data.tasks.map((task) => task.id);
    expect(ids.includes(specificTask.id)).toBe(true);
    expect(ids.includes(otherTask.id)).toBe(false);
  });

  it("search matches description via fielded query", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const describedTask = okData<{ task: { id: string } }>(
      (
        await runJson(repo, [
          "create",
          "Description target",
          "--description",
          "Investigate OAuth callback mismatch",
        ])
      ).envelope,
    ).task;
    const otherTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Unrelated"])).envelope,
    ).task;

    const result = await runJson(repo, ["search", "description:oauth"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ tasks: Array<{ id: string }> }>(result.envelope);
    const ids = data.tasks.map((task) => task.id);
    expect(ids.includes(describedTask.id)).toBe(true);
    expect(ids.includes(otherTask.id)).toBe(false);
  });

  it("search matches notes via fielded and bare text query", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const notedTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Note target"])).envelope,
    ).task;
    const otherTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "No note target"])).envelope,
    ).task;

    await runJson(repo, ["note", "add", notedTask.id, "Timeout while syncing release tasks"]);

    const notesFieldResult = await runJson(repo, ["search", "notes:timeout"]);
    expect(notesFieldResult.exitCode).toBe(0);
    const fieldIds = okData<{ tasks: Array<{ id: string }> }>(notesFieldResult.envelope).tasks.map(
      (task) => task.id,
    );
    expect(fieldIds.includes(notedTask.id)).toBe(true);
    expect(fieldIds.includes(otherTask.id)).toBe(false);

    const bareResult = await runJson(repo, ["search", "timeout"]);
    expect(bareResult.exitCode).toBe(0);
    const bareIds = okData<{ tasks: Array<{ id: string }> }>(bareResult.envelope).tasks.map(
      (task) => task.id,
    );
    expect(bareIds.includes(notedTask.id)).toBe(true);
    expect(bareIds.includes(otherTask.id)).toBe(false);
  });

  it("search returns empty list when no tasks match", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    await runJson(repo, ["create", "Some task"]);

    const result = await runJson(repo, ["search", "xyznonexistentquery123"]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ tasks: Array<{ id: string }> }>(result.envelope);
    expect(Array.isArray(data.tasks)).toBe(true);
    expect(data.tasks.length).toBe(0);
  });

  it("search JSON envelope has correct shape with tasks array", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string; title: string } }>(
      (await runJson(repo, ["create", "Envelope shape task"])).envelope,
    ).task;

    const result = await runJson(repo, ["search", "Envelope"]);
    expect(result.exitCode).toBe(0);
    assertEnvelopeShape(result.envelope);
    expect(result.envelope.command).toBe("tsq search");

    const data = okData<{ tasks: Task[] }>(result.envelope);
    expect(Array.isArray(data.tasks)).toBe(true);
    expect(data.tasks.length).toBeGreaterThan(0);

    const found = data.tasks.find((task) => task.id === created.id);
    expect(found).toBeDefined();
    expect(typeof found?.id).toBe("string");
    expect(typeof found?.title).toBe("string");
    expect(typeof found?.status).toBe("string");
    expect(typeof found?.kind).toBe("string");
    expect(typeof found?.priority).toBe("number");
    expect(Array.isArray(found?.labels)).toBe(true);
  });

  it("search by id prefix returns tasks whose id starts with the given prefix", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Id prefix task"])).envelope,
    ).task;

    const prefix = created.id.slice(0, 7);
    const result = await runJson(repo, ["search", `id:${prefix}`]);
    expect(result.exitCode).toBe(0);
    const data = okData<{ tasks: Array<{ id: string }> }>(result.envelope);
    const ids = data.tasks.map((task) => task.id);
    expect(ids.includes(created.id)).toBe(true);
  });
});
