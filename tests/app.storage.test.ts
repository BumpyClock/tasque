import { afterEach, describe, expect, it } from "bun:test";
import { readFile, readdir } from "node:fs/promises";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  appendEvents,
  ensureEventsFile,
  ensureTasqueGitignore,
  evaluateTaskSpec,
  loadProjectedState,
  persistProjection,
  sha256,
  writeDefaultConfig,
  writeTaskSpecAtomic,
} from "../src/app/storage";
import { applyEvents } from "../src/domain/projector";
import { createEmptyState } from "../src/domain/state";
import type { EventRecord, Task } from "../src/types";

const repos: string[] = [];

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-storage-"));
  repos.push(repo);
  return repo;
}

afterEach(async () => {
  await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
});

const at = (offset: number): string => `2026-02-17T00:00:0${offset}.000Z`;

const ev = (
  type: EventRecord["type"],
  taskId: string,
  payload: Record<string, unknown>,
  offset: number,
): EventRecord => ({
  event_id: `01ARZ3NDEKTSV4RRFFQ69G5FA${offset}`,
  ts: at(offset),
  actor: "test",
  type,
  task_id: taskId,
  payload,
});

describe("storage adapter: loadProjectedState returns correct state after event replay", () => {
  it("projects tasks from events written via appendEvents", async () => {
    const repo = await makeRepo();
    await ensureEventsFile(repo);

    const events: EventRecord[] = [
      ev("task.created", "tsq-aaa111", { title: "Alpha", kind: "task", priority: 1 }, 1),
      ev("task.created", "tsq-bbb222", { title: "Beta", kind: "task", priority: 2 }, 2),
      ev("task.updated", "tsq-aaa111", { status: "in_progress" }, 3),
    ];
    await appendEvents(repo, events);

    const { state, allEvents } = await loadProjectedState(repo);

    expect(allEvents).toHaveLength(3);
    expect(Object.keys(state.tasks)).toHaveLength(2);
    expect(state.tasks["tsq-aaa111"]?.status).toBe("in_progress");
    expect(state.tasks["tsq-bbb222"]?.status).toBe("open");
    expect(state.applied_events).toBe(3);
  });
});

describe("storage adapter: persistProjection writes cache and triggers snapshot", () => {
  it("writes state cache that subsequent loads can use incrementally", async () => {
    const repo = await makeRepo();
    await ensureEventsFile(repo);
    await writeDefaultConfig(repo);

    const events: EventRecord[] = [
      ev("task.created", "tsq-ccc333", { title: "Cache test", kind: "task", priority: 0 }, 1),
    ];
    await appendEvents(repo, events);

    const projected = applyEvents(createEmptyState(), events);
    await persistProjection(repo, projected, 1);

    const cacheRaw = await readFile(join(repo, ".tasque", "tasks.jsonl"), "utf8");
    const cached = JSON.parse(cacheRaw);
    expect(cached.applied_events).toBe(1);
    expect(cached.tasks["tsq-ccc333"]?.title).toBe("Cache test");

    const moreEvents: EventRecord[] = [
      ev("task.updated", "tsq-ccc333", { title: "Cache test updated" }, 2),
    ];
    await appendEvents(repo, moreEvents);

    const { state } = await loadProjectedState(repo);
    expect(state.tasks["tsq-ccc333"]?.title).toBe("Cache test updated");
    expect(state.applied_events).toBe(2);
  });

  it("creates a snapshot file when event count hits the configured interval", async () => {
    const repo = await makeRepo();
    await ensureEventsFile(repo);

    const configPath = join(repo, ".tasque", "config.json");
    await Bun.write(configPath, JSON.stringify({ schema_version: 1, snapshot_every: 2 }));

    const events: EventRecord[] = [
      ev("task.created", "tsq-ddd444", { title: "Snap A", kind: "task", priority: 1 }, 1),
      ev("task.created", "tsq-eee555", { title: "Snap B", kind: "task", priority: 1 }, 2),
    ];
    await appendEvents(repo, events);
    const projected = applyEvents(createEmptyState(), events);

    await persistProjection(repo, projected, 2);

    const snapshotsDir = join(repo, ".tasque", "snapshots");
    const entries = await readdir(snapshotsDir);
    const snapFiles = entries.filter((name) => name.endsWith(".json"));
    expect(snapFiles.length).toBeGreaterThanOrEqual(1);

    const snapContent = JSON.parse(await readFile(join(snapshotsDir, snapFiles[0]!), "utf8"));
    expect(snapContent.event_count).toBe(2);
    expect(snapContent.state.tasks["tsq-ddd444"]).toBeDefined();
    expect(snapContent.state.tasks["tsq-eee555"]).toBeDefined();
  });
});

describe("storage adapter: spec file operations", () => {
  it("writeTaskSpecAtomic creates spec file and evaluateTaskSpec validates it", async () => {
    const repo = await makeRepo();

    const specContent = [
      "# Overview",
      "Spec overview content.",
      "",
      "# Constraints / Non-goals",
      "None.",
      "",
      "# Interfaces (CLI/API)",
      "CLI only.",
      "",
      "# Data model / schema changes",
      "No changes.",
      "",
      "# Acceptance criteria",
      "- All tests pass.",
      "",
      "# Test plan",
      "- Run bun test.",
    ].join("\n");

    const result = await writeTaskSpecAtomic(repo, "tsq-fff666", specContent);
    expect(result.specPath).toBe(".tasque/specs/tsq-fff666/spec.md");
    expect(result.content).toBe(specContent);

    const fingerprint = sha256(specContent);
    const task: Task = {
      id: "tsq-fff666",
      kind: "task",
      title: "Test task",
      notes: [],
      status: "open",
      priority: 1,
      labels: [],
      created_at: at(1),
      updated_at: at(1),
      spec_path: result.specPath,
      spec_fingerprint: fingerprint,
    };

    const check = await evaluateTaskSpec(repo, "tsq-fff666", task);
    expect(check.ok).toBe(true);
    expect(check.diagnostics).toHaveLength(0);
    expect(check.spec.attached).toBe(true);
    expect(check.spec.missing_sections).toHaveLength(0);
  });

  it("evaluateTaskSpec reports missing spec when task has no spec metadata", async () => {
    const repo = await makeRepo();
    const task: Task = {
      id: "tsq-ggg777",
      kind: "task",
      title: "No spec task",
      notes: [],
      status: "open",
      priority: 1,
      labels: [],
      created_at: at(1),
      updated_at: at(1),
    };

    const check = await evaluateTaskSpec(repo, "tsq-ggg777", task);
    expect(check.ok).toBe(false);
    expect(check.diagnostics.some((d) => d.code === "SPEC_NOT_ATTACHED")).toBe(true);
  });
});

describe("storage adapter: ensureTasqueGitignore creates expected file", () => {
  it("writes a .gitignore with lock and snapshot ignore patterns", async () => {
    const repo = await makeRepo();
    await ensureEventsFile(repo);
    await ensureTasqueGitignore(repo);

    const content = await readFile(join(repo, ".tasque", ".gitignore"), "utf8");
    expect(content).toContain(".lock");
    expect(content).toContain("snapshots/");
    expect(content).toContain("tasks.jsonl");
  });
});
