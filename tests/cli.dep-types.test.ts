import { afterEach, describe, expect, it } from "bun:test";
import { cleanupRepos, makeRepo as makeRepoBase, okData, runJson as runJsonBase } from "./helpers";

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-dep-types-");
}

afterEach(cleanupRepos);

async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "dep-types-test");
}

describe("cli dependency types", () => {
  it("supports typed add/remove and keeps non-blocking starts_after out of ready gating", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const blocker = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Blocker"])).envelope,
    ).task;
    const child = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Child"])).envelope,
    ).task;

    await runJson(repo, ["dep", "add", child.id, blocker.id, "--type", "starts_after"]);
    let shown = okData<{
      blocker_edges: Array<{ id: string; dep_type: "blocks" | "starts_after" }>;
    }>((await runJson(repo, ["show", child.id])).envelope);
    expect(shown.blocker_edges).toContainEqual({ id: blocker.id, dep_type: "starts_after" });

    let ready = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["ready"])).envelope,
    ).tasks;
    expect(ready.map((task) => task.id)).toContain(child.id);

    await runJson(repo, ["dep", "add", child.id, blocker.id, "--type", "blocks"]);
    shown = okData<{ blocker_edges: Array<{ id: string; dep_type: "blocks" | "starts_after" }> }>(
      (await runJson(repo, ["show", child.id])).envelope,
    );
    expect(shown.blocker_edges).toContainEqual({ id: blocker.id, dep_type: "starts_after" });
    expect(shown.blocker_edges).toContainEqual({ id: blocker.id, dep_type: "blocks" });

    ready = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["ready"])).envelope,
    ).tasks;
    expect(ready.map((task) => task.id)).not.toContain(child.id);

    await runJson(repo, ["dep", "remove", child.id, blocker.id, "--type", "blocks"]);
    shown = okData<{ blocker_edges: Array<{ id: string; dep_type: "blocks" | "starts_after" }> }>(
      (await runJson(repo, ["show", child.id])).envelope,
    );
    expect(shown.blocker_edges).toEqual([{ id: blocker.id, dep_type: "starts_after" }]);

    ready = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["ready"])).envelope,
    ).tasks;
    expect(ready.map((task) => task.id)).toContain(child.id);
  });

  it("filters list by dependency type and direction", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const a = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "A"])).envelope,
    ).task;
    const b = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "B"])).envelope,
    ).task;
    const c = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "C"])).envelope,
    ).task;

    await runJson(repo, ["dep", "add", b.id, a.id, "--type", "blocks"]);
    await runJson(repo, ["dep", "add", c.id, a.id, "--type", "starts_after"]);

    const blocksOut = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["list", "--dep-type", "blocks", "--dep-direction", "out"])).envelope,
    ).tasks;
    expect(blocksOut.map((task) => task.id)).toEqual([b.id]);

    const blocksIn = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["list", "--dep-type", "blocks", "--dep-direction", "in"])).envelope,
    ).tasks;
    expect(blocksIn.map((task) => task.id)).toEqual([a.id]);

    const startsAny = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["list", "--dep-type", "starts_after", "--dep-direction", "any"]))
        .envelope,
    ).tasks;
    const startsAnyIds = startsAny.map((task) => task.id);
    expect(startsAnyIds).toContain(a.id);
    expect(startsAnyIds).toContain(c.id);
    expect(startsAnyIds).toHaveLength(2);

    const directionOnly = await runJson(repo, ["list", "--dep-direction", "in"]);
    expect(directionOnly.exitCode).toBe(1);
    expect(directionOnly.envelope.ok).toBe(false);
    expect(directionOnly.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("supports dep_type_in/out search fields and validates dep-type query tokens", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const a = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "A"])).envelope,
    ).task;
    const b = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "B"])).envelope,
    ).task;
    const c = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "C"])).envelope,
    ).task;

    await runJson(repo, ["dep", "add", b.id, a.id, "--type", "blocks"]);
    await runJson(repo, ["dep", "add", c.id, a.id, "--type", "starts_after"]);

    const inBlocks = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["search", "dep_type_in:blocks"])).envelope,
    ).tasks;
    expect(inBlocks.map((task) => task.id)).toEqual([a.id]);

    const outStartsAfter = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["search", "dep_type_out:starts_after"])).envelope,
    ).tasks;
    expect(outStartsAfter.map((task) => task.id)).toEqual([c.id]);

    const ambiguous = await runJson(repo, ["search", "dep_type:blocks"]);
    expect(ambiguous.exitCode).toBe(1);
    expect(ambiguous.envelope.ok).toBe(false);
    expect(ambiguous.envelope.error?.code).toBe("VALIDATION_ERROR");

    const invalid = await runJson(repo, ["search", "dep_type_in:nope"]);
    expect(invalid.exitCode).toBe(1);
    expect(invalid.envelope.ok).toBe(false);
    expect(invalid.envelope.error?.code).toBe("VALIDATION_ERROR");
  });
});
