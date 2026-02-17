import { afterEach, describe, expect, it } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { loadProjectedState } from "../src/app/state";
import { applyEvents } from "../src/domain/projector";
import { createEmptyState } from "../src/domain/state";
import { appendEvents } from "../src/store/events";
import { writeSnapshot } from "../src/store/snapshots";
import { writeStateCache } from "../src/store/state";
import type { EventRecord } from "../src/types";

const repos: string[] = [];

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-app-state-"));
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

describe("loadProjectedState incremental replay", () => {
  it("replays only new events when state cache is stale", async () => {
    const repo = await makeRepo();
    const firstBatch: EventRecord[] = [
      ev("task.created", "tsq-aaa111", { title: "A", kind: "task", priority: 1 }, 1),
      ev("task.created", "tsq-bbb222", { title: "B", kind: "task", priority: 1 }, 2),
    ];
    const secondBatch: EventRecord[] = [
      ev("task.updated", "tsq-aaa111", { status: "in_progress" }, 3),
    ];

    // Write all events
    await appendEvents(repo, [...firstBatch, ...secondBatch]);

    // Write a stale cache (only 2 events applied)
    const staleState = applyEvents(createEmptyState(), firstBatch);
    staleState.applied_events = 2;
    await writeStateCache(repo, staleState);

    // Load should incrementally apply the 3rd event
    const { state } = await loadProjectedState(repo);
    expect(state.tasks["tsq-aaa111"]?.status).toBe("in_progress");
    expect(state.applied_events).toBe(3);
  });

  it("falls back to snapshot when no cache exists", async () => {
    const repo = await makeRepo();
    const events: EventRecord[] = [
      ev("task.created", "tsq-aaa111", { title: "A", kind: "task", priority: 1 }, 1),
      ev("task.created", "tsq-bbb222", { title: "B", kind: "task", priority: 1 }, 2),
      ev("task.updated", "tsq-bbb222", { title: "B updated" }, 3),
    ];

    await appendEvents(repo, events);

    // Write snapshot at event 2
    const snapState = applyEvents(createEmptyState(), events.slice(0, 2));
    snapState.applied_events = 2;
    await writeSnapshot(repo, {
      taken_at: new Date().toISOString(),
      event_count: 2,
      state: snapState,
    });

    const { state, snapshot } = await loadProjectedState(repo);
    expect(snapshot).not.toBeNull();
    expect(state.tasks["tsq-bbb222"]?.title).toBe("B updated");
    expect(state.applied_events).toBe(3);
  });

  it("returns cached state directly when cache is fresh", async () => {
    const repo = await makeRepo();
    const events: EventRecord[] = [
      ev("task.created", "tsq-aaa111", { title: "A", kind: "task", priority: 1 }, 1),
    ];

    await appendEvents(repo, events);

    const freshState = applyEvents(createEmptyState(), events);
    freshState.applied_events = 1;
    await writeStateCache(repo, freshState);

    const { state } = await loadProjectedState(repo);
    expect(state.applied_events).toBe(1);
    expect(state.tasks["tsq-aaa111"]?.title).toBe("A");
  });
});
