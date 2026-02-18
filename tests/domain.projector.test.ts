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

describe("projector applyEvents parity", () => {
  test("applyEvents produces identical state to chained applyEvent calls", () => {
    const events: EventRecord[] = [
      event("task.created", "tsq-aaa111", { title: "A", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-bbb222", { title: "B", kind: "feature", priority: 2 }, 2),
      event("task.updated", "tsq-aaa111", { status: "in_progress" }, 3),
      event("dep.added", "tsq-bbb222", { blocker: "tsq-aaa111" }, 4),
      event("link.added", "tsq-aaa111", { target: "tsq-bbb222", type: "relates_to" }, 5),
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
      event("task.created", "tsq-clm001", { title: "Open task", kind: "task", priority: 1 }, 1),
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
      event("task.created", "tsq-clm002", { title: "Started task", kind: "task", priority: 1 }, 1),
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
      event("task.created", "tsq-clm003", { title: "Blocked task", kind: "task", priority: 1 }, 1),
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
      event("task.created", "tsq-clm004", { title: "Closed task", kind: "task", priority: 1 }, 1),
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
      event("task.created", "tsq-clm005", { title: "Canceled task", kind: "task", priority: 1 }, 1),
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
      event("task.created", "tsq-clm006", { title: "No assignee", kind: "task", priority: 1 }, 1),
    ]);

    const claimed = applyEvent(withTask, event("task.claimed", "tsq-clm006", {}, 2));

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

describe("projector rich content", () => {
  test("task.created stores description and initializes notes", () => {
    const state = applyEvent(
      createEmptyState(),
      event(
        "task.created",
        "tsq-rich01",
        { title: "Rich", kind: "task", priority: 1, description: "Initial context" },
        1,
      ),
    );

    expect(state.tasks["tsq-rich01"]?.description).toBe("Initial context");
    expect(state.tasks["tsq-rich01"]?.notes).toEqual([]);
  });

  test("task.updated can set and clear description", () => {
    const withTask = applyEvent(
      createEmptyState(),
      event(
        "task.created",
        "tsq-rich02",
        { title: "Update description", kind: "task", priority: 1 },
        1,
      ),
    );
    const withDescription = applyEvent(
      withTask,
      event("task.updated", "tsq-rich02", { description: "Details" }, 2),
    );
    const cleared = applyEvent(
      withDescription,
      event("task.updated", "tsq-rich02", { clear_description: true }, 3),
    );

    expect(withDescription.tasks["tsq-rich02"]?.description).toBe("Details");
    expect(cleared.tasks["tsq-rich02"]?.description).toBeUndefined();
  });

  test("task.noted appends deterministic note metadata", () => {
    const withTask = applyEvent(
      createEmptyState(),
      event("task.created", "tsq-rich03", { title: "Noted", kind: "task", priority: 1 }, 1),
    );
    const noted = applyEvent(
      withTask,
      event("task.noted", "tsq-rich03", { text: "First note" }, 2),
    );

    expect(noted.tasks["tsq-rich03"]?.notes).toEqual([
      {
        event_id: "01ARZ3NDEKTSV4RRFFQ69G5FA2",
        ts: at(2),
        actor: "test",
        text: "First note",
      },
    ]);
    expect(noted.tasks["tsq-rich03"]?.updated_at).toBe(at(2));
  });

  test("task.spec_attached stores spec metadata", () => {
    const withTask = applyEvent(
      createEmptyState(),
      event("task.created", "tsq-rich04", { title: "Spec", kind: "task", priority: 1 }, 1),
    );
    const attached = applyEvent(
      withTask,
      event(
        "task.spec_attached",
        "tsq-rich04",
        {
          spec_path: ".tasque/specs/tsq-rich04/spec.md",
          spec_fingerprint: "abc123",
          spec_attached_at: at(2),
          spec_attached_by: "spec-agent",
        },
        2,
      ),
    );

    expect(attached.tasks["tsq-rich04"]?.spec_path).toBe(".tasque/specs/tsq-rich04/spec.md");
    expect(attached.tasks["tsq-rich04"]?.spec_fingerprint).toBe("abc123");
    expect(attached.tasks["tsq-rich04"]?.spec_attached_at).toBe(at(2));
    expect(attached.tasks["tsq-rich04"]?.spec_attached_by).toBe("spec-agent");
    expect(attached.tasks["tsq-rich04"]?.updated_at).toBe(at(2));
  });

  test("task.spec_attached requires path and fingerprint", () => {
    const withTask = applyEvent(
      createEmptyState(),
      event("task.created", "tsq-rich05", { title: "Spec invalid", kind: "task", priority: 1 }, 1),
    );
    expect(() =>
      applyEvent(withTask, event("task.spec_attached", "tsq-rich05", { spec_path: "x" }, 2)),
    ).toThrow("task.spec_attached requires spec_path and spec_fingerprint");
  });
});

describe("projector spec attach", () => {
  test("task.spec_attached defaults attached metadata to event timestamp and actor", () => {
    const created = applyEvent(
      createEmptyState(),
      event("task.created", "tsq-spec01", { title: "Spec", kind: "task", priority: 1 }, 1),
    );

    const withSpec = applyEvent(
      created,
      event(
        "task.spec_attached",
        "tsq-spec01",
        {
          spec_path: ".tasque/specs/tsq-spec01/spec.md",
          spec_fingerprint: "abc123",
        },
        2,
      ),
    );

    const projected = withSpec.tasks["tsq-spec01"];
    expect(projected?.spec_path).toBe(".tasque/specs/tsq-spec01/spec.md");
    expect(projected?.spec_fingerprint).toBe("abc123");
    expect(projected?.spec_attached_at).toBe(at(2));
    expect(projected?.spec_attached_by).toBe("test");
    expect(projected?.updated_at).toBe(at(2));
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

describe("projector duplicate updates", () => {
  test("task.updated can set duplicate_of while preserving dependencies", () => {
    const initial = createEmptyState();
    const withTasks = applyEvents(initial, [
      event("task.created", "tsq-src001", { title: "Source", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-can001", { title: "Canonical", kind: "task", priority: 1 }, 2),
      event("task.created", "tsq-child2", { title: "Child", kind: "task", priority: 1 }, 3),
      event("dep.added", "tsq-child2", { blocker: "tsq-src001" }, 4),
    ]);

    const duplicated = applyEvent(
      withTasks,
      event("task.updated", "tsq-src001", { duplicate_of: "tsq-can001", status: "closed" }, 5),
    );

    expect(duplicated.tasks["tsq-src001"]?.duplicate_of).toBe("tsq-can001");
    expect(duplicated.tasks["tsq-src001"]?.status).toBe("closed");
    expect(duplicated.tasks["tsq-src001"]?.closed_at).toBe(at(5));
    expect(duplicated.deps["tsq-child2"]).toEqual(["tsq-src001"]);
  });
});
