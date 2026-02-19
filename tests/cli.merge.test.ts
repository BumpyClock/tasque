import { afterEach, describe, expect, it } from "bun:test";
import { SCHEMA_VERSION } from "../src/types";
import {
  cleanupRepos,
  makeRepo as makeRepoBase,
  okData,
  runCli as runCliBase,
  runJson as runJsonBase,
} from "./helpers";

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-merge-");
}

afterEach(cleanupRepos);

async function runJson(repoDir: string, args: string[]) {
  return runJsonBase(repoDir, args, "test-merge");
}

function createTask(repo: string, title: string) {
  return runJson(repo, ["create", title]).then((r) =>
    okData<{ task: { id: string; status: string; title: string } }>(r.envelope),
  );
}

describe("cli merge workflow", () => {
  it("merge two sources into target — apply", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const target = (await createTask(repo, "Target task")).task;
    const src1 = (await createTask(repo, "Source one")).task;
    const src2 = (await createTask(repo, "Source two")).task;

    const result = await runJson(repo, [
      "merge",
      src1.id,
      src2.id,
      "--into",
      target.id,
      "--reason",
      "consolidated",
    ]);
    expect(result.exitCode).toBe(0);
    expect(result.envelope.ok).toBe(true);

    const data = okData<{
      merged: Array<{ id: string; status: string }>;
      target: { id: string; title: string; status: string };
      dry_run: boolean;
      warnings: string[];
    }>(result.envelope);

    expect(data.dry_run).toBe(false);
    expect(data.merged.length).toBe(2);
    expect(data.merged[0]?.id).toBe(src1.id);
    expect(data.merged[0]?.status).toBe("closed");
    expect(data.merged[1]?.id).toBe(src2.id);
    expect(data.merged[1]?.status).toBe("closed");
    expect(data.target.id).toBe(target.id);

    // Verify sources are actually closed with duplicate_of
    const shown1 = okData<{
      task: { status: string; duplicate_of?: string };
      links: Record<string, string[]>;
    }>((await runJson(repo, ["show", src1.id])).envelope);
    expect(shown1.task.status).toBe("closed");
    expect(shown1.task.duplicate_of).toBe(target.id);
    expect(shown1.links.duplicates).toEqual([target.id]);

    const shown2 = okData<{
      task: { status: string; duplicate_of?: string };
      links: Record<string, string[]>;
    }>((await runJson(repo, ["show", src2.id])).envelope);
    expect(shown2.task.status).toBe("closed");
    expect(shown2.task.duplicate_of).toBe(target.id);
    expect(shown2.links.duplicates).toEqual([target.id]);
  });

  it("merge --dry-run does not persist", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const target = (await createTask(repo, "Target")).task;
    const src = (await createTask(repo, "Source")).task;

    const result = await runJson(repo, ["merge", src.id, "--into", target.id, "--dry-run"]);
    expect(result.exitCode).toBe(0);

    const data = okData<{
      merged: Array<{ id: string; status: string }>;
      dry_run: boolean;
      plan_summary?: {
        requested_sources: number;
        merged_sources: number;
        skipped_sources: number;
        planned_events: number;
      };
      projected?: {
        target: { id: string; status: string };
        sources: Array<{ id: string; status: string; duplicate_of?: string }>;
      };
    }>(result.envelope);
    expect(data.dry_run).toBe(true);
    expect(data.merged.length).toBe(1);
    expect(data.plan_summary).toBeDefined();
    expect(data.plan_summary?.requested_sources).toBe(1);
    expect(data.plan_summary?.merged_sources).toBe(1);
    expect(data.plan_summary?.skipped_sources).toBe(0);
    expect(data.plan_summary?.planned_events).toBe(3);
    expect(data.projected?.target.id).toBe(target.id);
    expect(data.projected?.sources.find((s) => s.id === src.id)?.status).toBe("closed");
    expect(data.projected?.sources.find((s) => s.id === src.id)?.duplicate_of).toBe(target.id);

    // Verify source is still open
    const shown = okData<{ task: { status: string; duplicate_of?: string } }>(
      (await runJson(repo, ["show", src.id])).envelope,
    );
    expect(shown.task.status).toBe("open");
    expect(shown.task.duplicate_of).toBeUndefined();
  });

  it("merge into closed target fails without --force", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const target = (await createTask(repo, "Target")).task;
    const src = (await createTask(repo, "Source")).task;

    await runJson(repo, ["close", target.id]);

    const result = await runJson(repo, ["merge", src.id, "--into", target.id]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
    expect(result.envelope.error?.message).toContain("--force");
  });

  it("merge into closed target with --force succeeds", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const target = (await createTask(repo, "Target")).task;
    const src = (await createTask(repo, "Source")).task;

    await runJson(repo, ["close", target.id]);

    const result = await runJson(repo, ["merge", src.id, "--into", target.id, "--force"]);
    expect(result.exitCode).toBe(0);
    expect(result.envelope.ok).toBe(true);

    const data = okData<{
      warnings: string[];
      merged: Array<{ id: string; status: string }>;
    }>(result.envelope);
    expect(data.merged.length).toBe(1);
    expect(data.warnings.some((w) => w.includes("forced"))).toBe(true);
  });

  it("merge with already-closed source skips gracefully", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const target = (await createTask(repo, "Target")).task;
    const src1 = (await createTask(repo, "Source open")).task;
    const src2 = (await createTask(repo, "Source closed")).task;

    await runJson(repo, ["close", src2.id]);

    const result = await runJson(repo, ["merge", src1.id, src2.id, "--into", target.id]);
    expect(result.exitCode).toBe(0);

    const data = okData<{
      merged: Array<{ id: string; status: string }>;
      warnings: string[];
    }>(result.envelope);

    // Only src1 gets merged; src2 is skipped with a warning
    expect(data.merged.length).toBe(1);
    expect(data.merged[0]?.id).toBe(src1.id);
    expect(data.warnings.some((w) => w.includes(src2.id) && w.includes("skipped"))).toBe(true);
  });

  it("merge source=target is rejected", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const task = (await createTask(repo, "Task")).task;

    const result = await runJson(repo, ["merge", task.id, "--into", task.id]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("merge with no sources fails", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const target = (await createTask(repo, "Target")).task;

    // Commander requires at least 1 variadic argument; without sources it fails
    const result = await runCliBase(repo, ["merge", "--into", target.id], "test-merge");
    expect(result.exitCode).not.toBe(0);
  });

  it("json envelope contract", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const target = (await createTask(repo, "Target")).task;
    const src = (await createTask(repo, "Source")).task;

    const result = await runJson(repo, ["merge", src.id, "--into", target.id]);
    expect(result.exitCode).toBe(0);
    expect(result.envelope.schema_version).toBe(SCHEMA_VERSION);
    expect(result.envelope.command).toBe("tsq merge");
    expect(result.envelope.ok).toBe(true);
    expect(result.envelope.data).toBeDefined();
  });
});
