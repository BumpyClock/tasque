import { afterEach, describe, expect, it } from "bun:test";
import { cleanupRepos, makeRepo as makeRepoBase, okData, runJson as runJsonBase } from "./helpers";

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-duplicate-");
}

afterEach(cleanupRepos);

async function runJson(repoDir: string, args: string[], actor = "duplicate-test") {
  return runJsonBase(repoDir, args, actor);
}

describe("cli duplicate workflow", () => {
  it("duplicate closes source, sets duplicate_of, and keeps dependencies unchanged", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const blocker = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Blocker task"])).envelope,
    ).task;
    const canonical = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Canonical task"])).envelope,
    ).task;
    const duplicate = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Duplicate task"])).envelope,
    ).task;

    await runJson(repo, ["dep", "add", duplicate.id, blocker.id]);

    const marked = await runJson(repo, [
      "duplicate",
      duplicate.id,
      "--of",
      canonical.id,
      "--reason",
      "same implementation target",
    ]);
    expect(marked.exitCode).toBe(0);
    const task = okData<{
      task: { id: string; status: string; duplicate_of?: string; closed_at?: string };
    }>(marked.envelope).task;
    expect(task.id).toBe(duplicate.id);
    expect(task.status).toBe("closed");
    expect(task.duplicate_of).toBe(canonical.id);
    expect(typeof task.closed_at).toBe("string");

    const shown = okData<{
      blockers: string[];
      links: Record<string, string[]>;
    }>((await runJson(repo, ["show", duplicate.id])).envelope);
    expect(shown.blockers).toEqual([blocker.id]);
    expect(shown.links.duplicates).toEqual([canonical.id]);
  });

  it("rejects duplicate self-edge", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const task = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Self"])).envelope,
    ).task;

    const result = await runJson(repo, ["duplicate", task.id, "--of", task.id]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("rejects duplicate cycle chains", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const alpha = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Alpha"])).envelope,
    ).task;
    const beta = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Beta"])).envelope,
    ).task;

    const first = await runJson(repo, ["duplicate", alpha.id, "--of", beta.id]);
    expect(first.exitCode).toBe(0);

    const second = await runJson(repo, ["duplicate", beta.id, "--of", alpha.id]);
    expect(second.exitCode).toBe(1);
    expect(second.envelope.ok).toBe(false);
    expect(second.envelope.error?.code).toBe("DUPLICATE_CYCLE");
  });

  it("rejects canceled canonical targets", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const source = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Source"])).envelope,
    ).task;
    const canonical = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Canonical"])).envelope,
    ).task;

    await runJson(repo, ["update", canonical.id, "--status", "canceled"]);
    const result = await runJson(repo, ["duplicate", source.id, "--of", canonical.id]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("INVALID_STATUS");
  });

  it("duplicates dry-run scaffold groups active tasks by normalized title", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const first = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Parser cleanup"])).envelope,
    ).task;
    const second = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "parser cleanup!!"])).envelope,
    ).task;
    await runJson(repo, ["create", "Unrelated item"]);

    const result = await runJson(repo, ["duplicates", "--limit", "10"]);
    expect(result.exitCode).toBe(0);
    expect(result.envelope.command).toBe("tsq duplicates");
    const data = okData<{
      scanned: number;
      groups: Array<{ key: string; tasks: Array<{ id: string }> }>;
    }>(result.envelope);
    expect(data.scanned).toBeGreaterThanOrEqual(3);
    expect(data.groups.length).toBeGreaterThanOrEqual(1);
    const parserGroup = data.groups.find((group) => group.key === "parser cleanup");
    expect(parserGroup).toBeDefined();
    expect(parserGroup?.tasks.map((task) => task.id)).toEqual([first.id, second.id]);
  });
});
