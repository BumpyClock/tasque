import { afterEach, describe, expect, it } from "bun:test";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import {
  cleanupRepos,
  cliCmd,
  makeRepo as makeRepoBase,
  okData,
  runJson as runJsonBase,
} from "./helpers";

async function makeRepo() {
  return makeRepoBase("tasque-create-enh-");
}
afterEach(cleanupRepos);
async function runJson(repoDir: string, args: string[], actor = "test-create", stdinText?: string) {
  return runJsonBase(repoDir, args, actor, stdinText);
}

describe("cli create --id", () => {
  it("creates task with explicit valid ID", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const result = await runJson(repo, ["create", "Explicit ID task", "--id", "tsq-abcd1234"]);
    expect(result.exitCode).toBe(0);
    const task = okData<{ task: { id: string } }>(result.envelope).task;
    expect(task.id).toBe("tsq-abcd1234");
  });

  it("rejects invalid explicit ID format", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const result = await runJson(repo, ["create", "Bad ID", "--id", "bad-id"]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
  });

  it("rejects duplicate explicit ID", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    await runJson(repo, ["create", "First", "--id", "tsq-abcd1234"]);
    const dup = await runJson(repo, ["create", "Second", "--id", "tsq-abcd1234"]);
    expect(dup.exitCode).toBe(1);
    expect(dup.envelope.ok).toBe(false);
  });

  it("rejects --id combined with --parent", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const parent = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Parent"])).envelope,
    ).task;
    const result = await runJson(repo, [
      "create",
      "Child",
      "--id",
      "tsq-abcd1234",
      "--parent",
      parent.id,
    ]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
  });
});

describe("cli create --body-file", () => {
  it("reads description from file", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const bodyPath = join(repo, "body.md");
    await writeFile(bodyPath, "This is the task body from a file.\n", "utf8");
    const result = await runJson(repo, ["create", "File body task", "--body-file", bodyPath]);
    expect(result.exitCode).toBe(0);
    const task = okData<{ task: { id: string; description: string } }>(result.envelope).task;
    expect(task.description).toContain("task body from a file");
  });

  it("reads description from stdin when --body-file - is used", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const stdinPath = join(repo, "stdin.txt");
    await writeFile(stdinPath, "This came from stdin.\n", "utf8");
    const proc = Bun.spawnSync({
      cmd: [...cliCmd, "create", "Stdin body task", "--body-file", "-", "--json"],
      cwd: repo,
      env: { ...process.env, TSQ_ACTOR: "test-create" },
      stdin: Bun.file(stdinPath),
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(proc.exitCode).toBe(0);
    const stdout = new TextDecoder().decode(proc.stdout).trim();
    const envelope = JSON.parse(stdout) as { ok: boolean; data?: unknown };
    expect(envelope.ok).toBe(true);
    const task = (envelope.data as { task: { id: string; description: string } }).task;
    expect(task.description).toContain("This came from stdin.");
  });

  it("rejects empty stdin body when --body-file - is used", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const result = await runJson(
      repo,
      ["create", "Empty stdin body", "--body-file", "-"],
      "test-create",
      "   \n",
    );
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.message).toContain("stdin content must not be empty");
  });

  it("rejects --description combined with --body-file", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const bodyPath = join(repo, "body.md");
    await writeFile(bodyPath, "content\n", "utf8");
    const result = await runJson(repo, [
      "create",
      "Conflict",
      "--description",
      "inline",
      "--body-file",
      bodyPath,
    ]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
  });

  it("rejects empty body file", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const bodyPath = join(repo, "empty.md");
    await writeFile(bodyPath, "   \n", "utf8");
    const result = await runJson(repo, ["create", "Empty body", "--body-file", bodyPath]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
  });
});
