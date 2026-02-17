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
      event(
        "task.created",
        "tsq-a11111",
        { title: "A", kind: "task", priority: 1 },
        1,
      ),
      event(
        "task.created",
        "tsq-b22222",
        { title: "B", kind: "task", priority: 1 },
        2,
      ),
    ]);

    const linked = applyEvent(
      withTasks,
      event(
        "link.added",
        "tsq-a11111",
        { target: "tsq-b22222", type: "relates_to" },
        3,
      ),
    );

    expect(linked.links["tsq-a11111"]?.relates_to).toEqual(["tsq-b22222"]);
    expect(linked.links["tsq-b22222"]?.relates_to).toEqual(["tsq-a11111"]);

    const unlinked = applyEvent(
      linked,
      event(
        "link.removed",
        "tsq-a11111",
        { target: "tsq-b22222", type: "relates_to" },
        4,
      ),
    );

    expect(unlinked.links["tsq-a11111"]?.relates_to ?? []).toEqual([]);
    expect(unlinked.links["tsq-b22222"]?.relates_to ?? []).toEqual([]);
  });
});

describe("projector applyEvents parity", () => {
  test("applyEvents produces identical state to chained applyEvent calls", () => {
    const events: EventRecord[] = [
      event(
        "task.created",
        "tsq-aaa111",
        { title: "A", kind: "task", priority: 1 },
        1,
      ),
      event(
        "task.created",
        "tsq-bbb222",
        { title: "B", kind: "feature", priority: 2 },
        2,
      ),
      event("task.updated", "tsq-aaa111", { status: "in_progress" }, 3),
      event("dep.added", "tsq-bbb222", { blocker: "tsq-aaa111" }, 4),
      event(
        "link.added",
        "tsq-aaa111",
        { target: "tsq-bbb222", type: "relates_to" },
        5,
      ),
      event("task.claimed", "tsq-aaa111", { assignee: "alice" }, 6),
    ];

    // Chained single-event application
    let chained = createEmptyState();
    for (const ev of events) {
      chained = applyEvent(chained, ev);
    }

    // Batch application
    const batched = applyEvents(createEmptyState(), events);

    expect(batched).toEqual(chained);
  });

  test("applyEvents with empty list returns base state unchanged", () => {
    const base = createEmptyState();
    const result = applyEvents(base, []);
    expect(result).toBe(base);
  });
});

describe("projector claim transitions", () => {
  test("claim on open task transitions status to in_progress", () => {
    const initial = createEmptyState();
    const withTask = applyEvents(initial, [
      event(
        "task.created",
        "tsq-clm001",
        { title: "Open task", kind: "task", priority: 1 },
        1,
      ),
    ]);

    expect(withTask.tasks["tsq-clm001"]?.status).toBe("open");

    const claimed = applyEvent(
      withTask,
      event("task.claimed", "tsq-clm001", { assignee: "alice" }, 2),
    );

    expect(claimed.tasks["tsq-clm001"]?.status).toBe("in_progress");
    expect(claimed.tasks["tsq-clm001"]?.assignee).toBe("alice");
  });

  test("claim on in_progress task preserves in_progress status", () => {
    const initial = createEmptyState();
    const withTask = applyEvents(initial, [
      event(
        "task.created",
        "tsq-clm002",
        { title: "Started task", kind: "task", priority: 1 },
        1,
      ),
      event("task.updated", "tsq-clm002", { status: "in_progress" }, 2),
    ]);

    expect(withTask.tasks["tsq-clm002"]?.status).toBe("in_progress");

    const claimed = applyEvent(
      withTask,
      event("task.claimed", "tsq-clm002", { assignee: "bob" }, 3),
    );

    expect(claimed.tasks["tsq-clm002"]?.status).toBe("in_progress");
    expect(claimed.tasks["tsq-clm002"]?.assignee).toBe("bob");
  });

  test("claim on blocked task preserves blocked status at projector level", () => {
    const initial = createEmptyState();
    const withTask = applyEvents(initial, [
      event(
        "task.created",
        "tsq-clm003",
        { title: "Blocked task", kind: "task", priority: 1 },
        1,
      ),
      event("task.updated", "tsq-clm003", { status: "blocked" }, 2),
    ]);

    expect(withTask.tasks["tsq-clm003"]?.status).toBe("blocked");

    // Projector doesn't reject — it preserves non-open statuses.
    // The service layer is responsible for rejecting blocked claims.
    const claimed = applyEvent(
      withTask,
      event("task.claimed", "tsq-clm003", { assignee: "carol" }, 3),
    );

    expect(claimed.tasks["tsq-clm003"]?.status).toBe("blocked");
    expect(claimed.tasks["tsq-clm003"]?.assignee).toBe("carol");
  });

  test("claim on closed task preserves closed status at projector level", () => {
    const initial = createEmptyState();
    const withTask = applyEvents(initial, [
      event(
        "task.created",
        "tsq-clm004",
        { title: "Closed task", kind: "task", priority: 1 },
        1,
      ),
      event("task.updated", "tsq-clm004", { status: "closed" }, 2),
    ]);

    expect(withTask.tasks["tsq-clm004"]?.status).toBe("closed");

    // Projector doesn't reject — service layer guards this.
    const claimed = applyEvent(
      withTask,
      event("task.claimed", "tsq-clm004", { assignee: "dave" }, 3),
    );

    expect(claimed.tasks["tsq-clm004"]?.status).toBe("closed");
    expect(claimed.tasks["tsq-clm004"]?.assignee).toBe("dave");
  });

  test("claim on canceled task preserves canceled status at projector level", () => {
    const initial = createEmptyState();
    const withTask = applyEvents(initial, [
      event(
        "task.created",
        "tsq-clm005",
        { title: "Canceled task", kind: "task", priority: 1 },
        1,
      ),
      event("task.updated", "tsq-clm005", { status: "canceled" }, 2),
    ]);

    expect(withTask.tasks["tsq-clm005"]?.status).toBe("canceled");

    // Projector doesn't reject — service layer guards this.
    const claimed = applyEvent(
      withTask,
      event("task.claimed", "tsq-clm005", { assignee: "eve" }, 3),
    );

    expect(claimed.tasks["tsq-clm005"]?.status).toBe("canceled");
    expect(claimed.tasks["tsq-clm005"]?.assignee).toBe("eve");
  });

  test("claim uses actor as fallback when payload has no assignee", () => {
    const initial = createEmptyState();
    const withTask = applyEvents(initial, [
      event(
        "task.created",
        "tsq-clm006",
        { title: "No assignee", kind: "task", priority: 1 },
        1,
      ),
    ]);

    const claimed = applyEvent(
      withTask,
      event("task.claimed", "tsq-clm006", {}, 2),
    );

    // The actor from the event is "test" (set by our helper)
    expect(claimed.tasks["tsq-clm006"]?.assignee).toBe("test");
    expect(claimed.tasks["tsq-clm006"]?.status).toBe("in_progress");
  });

  test("claim updates updated_at timestamp", () => {
    const initial = createEmptyState();
    const withTask = applyEvents(initial, [
      event(
        "task.created",
        "tsq-clm007",
        { title: "Timestamp check", kind: "task", priority: 1 },
        1,
      ),
    ]);

    const createdAt = withTask.tasks["tsq-clm007"]?.updated_at;

    const claimed = applyEvent(
      withTask,
      event("task.claimed", "tsq-clm007", { assignee: "frank" }, 5),
    );

    expect(claimed.tasks["tsq-clm007"]?.updated_at).toBe(at(5));
    expect(claimed.tasks["tsq-clm007"]?.updated_at).not.toBe(createdAt);
  });

  test("claim does not alter title, priority, or labels", () => {
    const initial = createEmptyState();
    const withTask = applyEvents(initial, [
      event(
        "task.created",
        "tsq-clm008",
        {
          title: "Preserve me",
          kind: "feature",
          priority: 2,
          labels: ["regression", "v1"],
        },
        1,
      ),
    ]);

    const before = withTask.tasks["tsq-clm008"];

    const claimed = applyEvent(
      withTask,
      event("task.claimed", "tsq-clm008", { assignee: "grace" }, 2),
    );

    const after = claimed.tasks["tsq-clm008"];
    expect(after?.title).toBe(before?.title);
    expect(after?.priority).toBe(before?.priority);
    expect(after?.labels).toEqual(before?.labels);
    expect(after?.kind).toBe(before?.kind);
  });
});

describe("projector supersede", () => {
  test("closes source task and sets superseded_by while leaving target unchanged", () => {
    const initial = createEmptyState();
    const withTasks = applyEvents(initial, [
      event(
        "task.created",
        "tsq-old001",
        { title: "Old", kind: "task", priority: 1 },
        1,
      ),
      event(
        "task.created",
        "tsq-new001",
        { title: "New", kind: "task", priority: 1 },
        2,
      ),
      event(
        "task.created",
        "tsq-child1",
        { title: "Child", kind: "task", priority: 1 },
        3,
      ),
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
