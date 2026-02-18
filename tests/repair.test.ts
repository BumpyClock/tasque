import { afterEach, describe, expect, it } from "bun:test";
import { access, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { type JsonResult, assertEnvelopeShape, cliCmd, okData } from "./helpers";

interface RepairPlan {
  orphaned_deps: Array<{ child: string; blocker: string }>;
  orphaned_links: Array<{ src: string; dst: string; type: string }>;
  stale_temps: string[];
  stale_lock: boolean;
  old_snapshots: string[];
}

interface RepairResult {
  plan: RepairPlan;
  applied: boolean;
  events_appended: number;
  files_removed: number;
}

const repos: string[] = [];

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-repair-"));
  repos.push(repo);
  return repo;
}

afterEach(async () => {
  await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
});

async function runJson(repoDir: string, args: string[]): Promise<JsonResult> {
  const proc = Bun.spawn({
    cmd: [...cliCmd, ...args, "--json"],
    cwd: repoDir,
    env: { ...process.env, TSQ_ACTOR: "repair-test" },
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
  const parsed = JSON.parse(trimmed) as unknown;
  assertEnvelopeShape(parsed);

  return { exitCode, stdout, stderr, envelope: parsed };
}

async function pathExists(path: string): Promise<boolean> {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

async function injectOrphanDep(repoDir: string, child: string, blocker: string): Promise<void> {
  const stateFile = join(repoDir, ".tasque", "state.json");
  const raw = await readFile(stateFile, "utf8");
  const state = JSON.parse(raw);
  const deps: string[] = state.deps[child] ?? [];
  if (!deps.includes(blocker)) {
    deps.push(blocker);
  }
  state.deps[child] = deps;
  await writeFile(stateFile, JSON.stringify(state, null, 2), "utf8");
}

async function injectOrphanLink(
  repoDir: string,
  src: string,
  target: string,
  type: string,
): Promise<void> {
  const stateFile = join(repoDir, ".tasque", "state.json");
  const raw = await readFile(stateFile, "utf8");
  const state = JSON.parse(raw);
  if (!state.links[src]) {
    state.links[src] = {};
  }
  const targets: string[] = state.links[src][type] ?? [];
  if (!targets.includes(target)) {
    targets.push(target);
  }
  state.links[src][type] = targets;
  await writeFile(stateFile, JSON.stringify(state, null, 2), "utf8");
}

describe("tsq repair", () => {
  it("clean repo returns empty plan", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const result = await runJson(repo, ["repair"]);

    expect(result.exitCode).toBe(0);
    const data = okData<RepairResult>(result.envelope);
    expect(data.plan.orphaned_deps).toEqual([]);
    expect(data.plan.orphaned_links).toEqual([]);
    expect(data.plan.stale_temps).toEqual([]);
    expect(data.plan.stale_lock).toBe(false);
    expect(data.plan.old_snapshots).toEqual([]);
    expect(data.applied).toBe(false);
  });

  it("dry-run detects orphaned deps", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = await runJson(repo, ["create", "Task with orphan dep"]);
    expect(created.exitCode).toBe(0);
    const taskId = okData<{ task: { id: string } }>(created.envelope).task.id;

    // Inject orphaned dep directly into state cache (bypasses projector validation)
    await injectOrphanDep(repo, taskId, "tsq-nonexistent");

    const result = await runJson(repo, ["repair"]);

    expect(result.exitCode).toBe(0);
    const data = okData<RepairResult>(result.envelope);
    expect(data.plan.orphaned_deps.length >= 1).toBe(true);
    expect(data.applied).toBe(false);
  });

  it("--fix removes orphaned deps", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = await runJson(repo, ["create", "Task to fix orphan dep"]);
    expect(created.exitCode).toBe(0);
    const taskId = okData<{ task: { id: string } }>(created.envelope).task.id;

    // Inject orphaned dep directly into state cache (bypasses projector validation)
    await injectOrphanDep(repo, taskId, "tsq-nonexistent");

    const result = await runJson(repo, ["repair", "--fix"]);

    expect(result.exitCode).toBe(0);
    const data = okData<RepairResult>(result.envelope);
    expect(data.applied).toBe(true);
    expect(data.events_appended >= 1).toBe(true);

    const doctor = await runJson(repo, ["doctor"]);
    expect(doctor.exitCode).toBe(0);
    const doctorData = okData<{ issues: string[] }>(doctor.envelope);
    expect(doctorData.issues.length).toBe(0);
  });

  it("dry-run detects orphaned links", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = await runJson(repo, ["create", "Task with orphan link"]);
    expect(created.exitCode).toBe(0);
    const taskId = okData<{ task: { id: string } }>(created.envelope).task.id;

    // Inject orphaned link directly into state cache (bypasses projector validation)
    await injectOrphanLink(repo, taskId, "tsq-nonexistent", "relates_to");

    const result = await runJson(repo, ["repair"]);

    expect(result.exitCode).toBe(0);
    const data = okData<RepairResult>(result.envelope);
    expect(data.plan.orphaned_links.length >= 1).toBe(true);
  });

  it("temp file cleanup detects and removes stale temp files", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const tasqueDir = join(repo, ".tasque");
    await writeFile(join(tasqueDir, "state.json.tmp-12345"), "{}", "utf8");
    await writeFile(join(tasqueDir, "events.jsonl.tmp-99999"), "{}", "utf8");

    const dryRun = await runJson(repo, ["repair"]);
    expect(dryRun.exitCode).toBe(0);
    const dryData = okData<RepairResult>(dryRun.envelope);
    expect(dryData.plan.stale_temps.length).toBe(2);

    const fixRun = await runJson(repo, ["repair", "--fix"]);
    expect(fixRun.exitCode).toBe(0);
    const fixData = okData<RepairResult>(fixRun.envelope);
    expect(fixData.files_removed >= 2).toBe(true);

    expect(await pathExists(join(tasqueDir, "state.json.tmp-12345"))).toBe(false);
    expect(await pathExists(join(tasqueDir, "events.jsonl.tmp-99999"))).toBe(false);
  });

  it("snapshot GC keeps last 5 and removes older ones", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const snapshotsDir = join(repo, ".tasque", "snapshots");
    await mkdir(snapshotsDir, { recursive: true });

    const validSnapshot = JSON.stringify({
      taken_at: "2025-01-01T00:00:00.000Z",
      event_count: 0,
      state: {
        tasks: {},
        deps: {},
        links: {},
        child_counters: {},
        created_order: [],
        applied_events: 0,
      },
    });

    const snapshotNames = [
      "2025-01-01T00-00-00-000Z-1.json",
      "2025-01-02T00-00-00-000Z-2.json",
      "2025-01-03T00-00-00-000Z-3.json",
      "2025-01-04T00-00-00-000Z-4.json",
      "2025-01-05T00-00-00-000Z-5.json",
      "2025-01-06T00-00-00-000Z-6.json",
      "2025-01-07T00-00-00-000Z-7.json",
      "2025-01-08T00-00-00-000Z-8.json",
    ];
    for (const name of snapshotNames) {
      await writeFile(join(snapshotsDir, name), validSnapshot, "utf8");
    }

    const dryRun = await runJson(repo, ["repair"]);
    expect(dryRun.exitCode).toBe(0);
    const dryData = okData<RepairResult>(dryRun.envelope);
    expect(dryData.plan.old_snapshots.length).toBe(3);

    const fixRun = await runJson(repo, ["repair", "--fix"]);
    expect(fixRun.exitCode).toBe(0);
    const fixData = okData<RepairResult>(fixRun.envelope);
    expect(fixData.files_removed >= 3).toBe(true);

    const remaining = await readdir(snapshotsDir);
    const jsonFiles = remaining.filter((name) => name.endsWith(".json"));
    expect(jsonFiles.length).toBe(5);
  });

  it("force-unlock removes stale lock file", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const lockFile = join(repo, ".tasque", ".lock");
    await writeFile(
      lockFile,
      JSON.stringify({ host: "other-machine", pid: 99999, created_at: "2025-01-01T00:00:00.000Z" }),
      "utf8",
    );

    const result = await runJson(repo, ["repair", "--fix", "--force-unlock"]);

    expect(result.exitCode).toBe(0);
    const data = okData<RepairResult>(result.envelope);
    expect(data.plan.stale_lock).toBe(true);
    expect(data.applied).toBe(true);
  });

  it("--force-unlock without --fix returns a validation error", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const result = await runJson(repo, ["repair", "--force-unlock"]);

    expect(result.exitCode).not.toBe(0);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });
});
