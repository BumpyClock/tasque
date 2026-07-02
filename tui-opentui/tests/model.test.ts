import { describe, expect, it } from "bun:test";
import {
  type TasqueTask,
  buildEpicProgressList,
  computeSummary,
  sortTasks,
} from "../src/model";

function makeTask(overrides: Partial<TasqueTask> & { id: string }): TasqueTask {
  return {
    kind: "task",
    title: overrides.id,
    status: "open",
    priority: 2,
    labels: [],
    created_at: "2026-01-01T00:00:00.000Z",
    updated_at: "2026-01-01T00:00:00.000Z",
    ...overrides,
  };
}

describe("buildEpicProgressList", () => {
  it("returns an empty list when there are no epics", () => {
    const tasks = [makeTask({ id: "tsq-1" }), makeTask({ id: "tsq-2" })];
    expect(buildEpicProgressList(tasks)).toEqual([]);
  });

  it("counts closed and canceled children as done for a single epic", () => {
    const tasks = [
      makeTask({ id: "tsq-1", kind: "epic" }),
      makeTask({ id: "tsq-1.1", parent_id: "tsq-1", status: "closed" }),
      makeTask({ id: "tsq-1.2", parent_id: "tsq-1", status: "canceled" }),
      makeTask({ id: "tsq-1.3", parent_id: "tsq-1", status: "in_progress" }),
      makeTask({ id: "tsq-1.4", parent_id: "tsq-1", status: "blocked" }),
      makeTask({ id: "tsq-1.5", parent_id: "tsq-1", status: "open" }),
      makeTask({ id: "tsq-1.6", parent_id: "tsq-1", status: "deferred" }),
    ];

    const progress = buildEpicProgressList(tasks);
    expect(progress).toHaveLength(1);
    expect(progress[0]?.epic.id).toBe("tsq-1");
    expect(progress[0]?.children).toHaveLength(6);
    expect(progress[0]?.done).toBe(2);
    expect(progress[0]?.inProgress).toBe(2);
    expect(progress[0]?.open).toBe(2);
  });

  it("returns one entry per epic with its own children", () => {
    const tasks = [
      makeTask({ id: "tsq-1", kind: "epic" }),
      makeTask({ id: "tsq-2", kind: "epic" }),
      makeTask({ id: "tsq-3", kind: "epic" }),
      makeTask({ id: "tsq-1.1", parent_id: "tsq-1", status: "closed" }),
      makeTask({ id: "tsq-2.1", parent_id: "tsq-2", status: "open" }),
      makeTask({ id: "tsq-2.2", parent_id: "tsq-2", status: "closed" }),
    ];

    const progress = buildEpicProgressList(tasks);
    expect(progress.map((entry) => entry.epic.id)).toEqual([
      "tsq-1",
      "tsq-2",
      "tsq-3",
    ]);
    expect(progress[0]?.done).toBe(1);
    expect(progress[1]?.children).toHaveLength(2);
    expect(progress[1]?.done).toBe(1);
    expect(progress[1]?.open).toBe(1);
    expect(progress[2]?.children).toHaveLength(0);
    expect(progress[2]?.done).toBe(0);
  });
});

describe("sortTasks", () => {
  it("orders by status, then priority, then created_at", () => {
    const tasks = [
      makeTask({ id: "tsq-1", status: "open", priority: 1 }),
      makeTask({ id: "tsq-2", status: "in_progress", priority: 3 }),
      makeTask({
        id: "tsq-3",
        status: "open",
        priority: 1,
        created_at: "2025-01-01T00:00:00.000Z",
      }),
      makeTask({ id: "tsq-4", status: "closed" }),
    ];

    const sorted = sortTasks(tasks).map((task) => task.id);
    expect(sorted).toEqual(["tsq-2", "tsq-3", "tsq-1", "tsq-4"]);
  });

  it("does not mutate the input array", () => {
    const tasks = [
      makeTask({ id: "tsq-1", status: "closed" }),
      makeTask({ id: "tsq-2", status: "in_progress" }),
    ];
    sortTasks(tasks);
    expect(tasks[0]?.id).toBe("tsq-1");
  });
});

describe("computeSummary", () => {
  it("counts open, in_progress, and blocked tasks", () => {
    const tasks = [
      makeTask({ id: "tsq-1", status: "open" }),
      makeTask({ id: "tsq-2", status: "in_progress" }),
      makeTask({ id: "tsq-3", status: "blocked" }),
      makeTask({ id: "tsq-4", status: "closed" }),
    ];

    expect(computeSummary(tasks)).toEqual({
      total: 4,
      open: 1,
      inProgress: 1,
      blocked: 1,
    });
  });

  it("returns zeroes for an empty list", () => {
    expect(computeSummary([])).toEqual({
      total: 0,
      open: 0,
      inProgress: 0,
      blocked: 0,
    });
  });
});
