import { describe, expect, test } from "bun:test";

import { applyEvent, applyEvents } from "../src/domain/projector";
import { createEmptyState } from "../src/domain/state";
import type { EventRecord } from "../src/types";

const at = (offset: number): string => `2026-02-17T00:00:0${offset}.000Z`;

const event = (
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

describe("projector links", () => {
  test("adds and removes relates_to bidirectionally", () => {
    const initial = createEmptyState();
    const withTasks = applyEvents(initial, [
      event("task.created", "tsq-a11111", { title: "A", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-b22222", { title: "B", kind: "task", priority: 1 }, 2),
    ]);

    const linked = applyEvent(
      withTasks,
      event("link.added", "tsq-a11111", { target: "tsq-b22222", type: "relates_to" }, 3),
    );

    expect(linked.links["tsq-a11111"]?.relates_to).toEqual(["tsq-b22222"]);
    expect(linked.links["tsq-b22222"]?.relates_to).toEqual(["tsq-a11111"]);

    const unlinked = applyEvent(
      linked,
      event("link.removed", "tsq-a11111", { target: "tsq-b22222", type: "relates_to" }, 4),
    );

    expect(unlinked.links["tsq-a11111"]?.relates_to ?? []).toEqual([]);
    expect(unlinked.links["tsq-b22222"]?.relates_to ?? []).toEqual([]);
  });
});

describe("projector supersede", () => {
  test("closes source task and sets superseded_by while leaving target unchanged", () => {
    const initial = createEmptyState();
    const withTasks = applyEvents(initial, [
      event("task.created", "tsq-old001", { title: "Old", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-new001", { title: "New", kind: "task", priority: 1 }, 2),
      event("task.created", "tsq-child1", { title: "Child", kind: "task", priority: 1 }, 3),
      event("dep.added", "tsq-child1", { blocker: "tsq-old001" }, 4),
    ]);

    const superseded = applyEvent(
      withTasks,
      event("task.superseded", "tsq-old001", { with: "tsq-new001" }, 5),
    );

    expect(superseded.tasks["tsq-old001"]?.status).toBe("closed");
    expect(superseded.tasks["tsq-old001"]?.superseded_by).toBe("tsq-new001");
    expect(superseded.tasks["tsq-old001"]?.closed_at).toBe(at(5));
    expect(superseded.tasks["tsq-new001"]?.status).toBe("open");
    expect(superseded.deps["tsq-child1"]).toEqual(["tsq-old001"]);
  });
});
