import { describe, expect, test } from "bun:test";

import { applyEvent, applyEvents } from "../src/domain/projector";
import { createEmptyState } from "../src/domain/state";
import { TsqError } from "../src/errors";
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
      event("task.status_set", "tsq-aaa111", { status: "in_progress" }, 3),
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
      event("task.status_set", "tsq-clm002", { status: "in_progress" }, 2),
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
      event("task.status_set", "tsq-clm003", { status: "blocked" }, 2),
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

  test("claim on closed task throws invalid transition", () => {
    const initial = createEmptyState();
    const withTask = applyEvents(initial, [
      event("task.created", "tsq-clm004", { title: "Closed task", kind: "task", priority: 1 }, 1),
      event("task.status_set", "tsq-clm004", { status: "closed" }, 2),
    ]);

    expect(withTask.tasks["tsq-clm004"]?.status).toBe("closed");

    expect(() =>
      applyEvent(withTask, event("task.claimed", "tsq-clm004", { assignee: "dave" }, 3)),
    ).toThrow(TsqError);
  });

  test("claim on canceled task throws invalid transition", () => {
    const initial = createEmptyState();
    const withTask = applyEvents(initial, [
      event("task.created", "tsq-clm005", { title: "Canceled task", kind: "task", priority: 1 }, 1),
      event("task.status_set", "tsq-clm005", { status: "canceled" }, 2),
    ]);

    expect(withTask.tasks["tsq-clm005"]?.status).toBe("canceled");

    expect(() =>
      applyEvent(withTask, event("task.claimed", "tsq-clm005", { assignee: "eve" }, 3)),
    ).toThrow(TsqError);
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
    expect(superseded.deps["tsq-child1"]).toEqual([{ blocker: "tsq-old001", dep_type: "blocks" }]);
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

    const withDupField = applyEvent(
      withTasks,
      event("task.updated", "tsq-src001", { duplicate_of: "tsq-can001" }, 5),
    );
    const duplicated = applyEvent(
      withDupField,
      event("task.status_set", "tsq-src001", { status: "closed" }, 6),
    );

    expect(duplicated.tasks["tsq-src001"]?.duplicate_of).toBe("tsq-can001");
    expect(duplicated.tasks["tsq-src001"]?.status).toBe("closed");
    expect(duplicated.tasks["tsq-src001"]?.closed_at).toBe(at(6));
    expect(duplicated.deps["tsq-child2"]).toEqual([{ blocker: "tsq-src001", dep_type: "blocks" }]);
  });
});

describe("projector cycle detection on dep.added", () => {
  test("rejects dep.added that would create a direct cycle", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-cyc001", { title: "A", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-cyc002", { title: "B", kind: "task", priority: 1 }, 2),
      event("dep.added", "tsq-cyc001", { blocker: "tsq-cyc002" }, 3),
    ]);

    expect(() =>
      applyEvent(withTasks, event("dep.added", "tsq-cyc002", { blocker: "tsq-cyc001" }, 4)),
    ).toThrow();
  });

  test("rejects dep.added that would create a transitive cycle", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-cyc003", { title: "A", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-cyc004", { title: "B", kind: "task", priority: 1 }, 2),
      event("task.created", "tsq-cyc005", { title: "C", kind: "task", priority: 1 }, 3),
      event("dep.added", "tsq-cyc003", { blocker: "tsq-cyc004" }, 4),
      event("dep.added", "tsq-cyc004", { blocker: "tsq-cyc005" }, 5),
    ]);

    try {
      applyEvent(withTasks, event("dep.added", "tsq-cyc005", { blocker: "tsq-cyc003" }, 6));
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("DEPENDENCY_CYCLE");
    }
  });

  test("rejects self-dependency via dep.added", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-cyc006", { title: "Self", kind: "task", priority: 1 }, 1),
    ]);

    try {
      applyEvent(withTask, event("dep.added", "tsq-cyc006", { blocker: "tsq-cyc006" }, 2));
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("DEPENDENCY_CYCLE");
    }
  });
});

describe("projector dep/link target validation", () => {
  test("dep.added rejects missing child task", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-dep001", { title: "Blocker", kind: "task", priority: 1 }, 1),
    ]);

    try {
      applyEvent(withTask, event("dep.added", "tsq-missing", { blocker: "tsq-dep001" }, 2));
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("TASK_NOT_FOUND");
    }
  });

  test("dep.added rejects missing blocker task", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-dep002", { title: "Child", kind: "task", priority: 1 }, 1),
    ]);

    try {
      applyEvent(withTask, event("dep.added", "tsq-dep002", { blocker: "tsq-missing" }, 2));
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("TASK_NOT_FOUND");
    }
  });

  test("dep.added succeeds when both child and blocker exist", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-dep003", { title: "Child", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-dep004", { title: "Blocker", kind: "task", priority: 1 }, 2),
    ]);

    const result = applyEvent(
      withTasks,
      event("dep.added", "tsq-dep003", { blocker: "tsq-dep004" }, 3),
    );

    expect(result.deps["tsq-dep003"]).toEqual([{ blocker: "tsq-dep004", dep_type: "blocks" }]);
  });

  test("link.added rejects missing source task", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-lnk001", { title: "Target", kind: "task", priority: 1 }, 1),
    ]);

    try {
      applyEvent(
        withTask,
        event("link.added", "tsq-missing", { target: "tsq-lnk001", type: "relates_to" }, 2),
      );
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("TASK_NOT_FOUND");
    }
  });

  test("link.added rejects missing target task", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-lnk002", { title: "Source", kind: "task", priority: 1 }, 1),
    ]);

    try {
      applyEvent(
        withTask,
        event("link.added", "tsq-lnk002", { target: "tsq-missing", type: "relates_to" }, 2),
      );
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("TASK_NOT_FOUND");
    }
  });

  test("link.added succeeds when both source and target exist", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-lnk003", { title: "Source", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-lnk004", { title: "Target", kind: "task", priority: 1 }, 2),
    ]);

    const result = applyEvent(
      withTasks,
      event("link.added", "tsq-lnk003", { target: "tsq-lnk004", type: "relates_to" }, 3),
    );

    expect(result.links["tsq-lnk003"]?.relates_to).toEqual(["tsq-lnk004"]);
    expect(result.links["tsq-lnk004"]?.relates_to).toEqual(["tsq-lnk003"]);
  });

  test("valid historical logs with deps and links still replay successfully", () => {
    const events: EventRecord[] = [
      event("task.created", "tsq-hist01", { title: "A", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-hist02", { title: "B", kind: "task", priority: 1 }, 2),
      event("task.created", "tsq-hist03", { title: "C", kind: "task", priority: 1 }, 3),
      event("dep.added", "tsq-hist02", { blocker: "tsq-hist01" }, 4),
      event("link.added", "tsq-hist01", { target: "tsq-hist03", type: "relates_to" }, 5),
      event("link.added", "tsq-hist02", { target: "tsq-hist03", type: "duplicates" }, 6),
    ];

    const result = applyEvents(createEmptyState(), events);

    expect(result.deps["tsq-hist02"]).toEqual([{ blocker: "tsq-hist01", dep_type: "blocks" }]);
    expect(result.links["tsq-hist01"]?.relates_to).toEqual(["tsq-hist03"]);
    expect(result.links["tsq-hist02"]?.duplicates).toEqual(["tsq-hist03"]);
  });
});

describe("projector empty title validation", () => {
  test("task.updated with empty string title throws validation error", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-ttl001", { title: "Original", kind: "task", priority: 1 }, 1),
    ]);

    try {
      applyEvent(withTask, event("task.updated", "tsq-ttl001", { title: "" }, 2));
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("INVALID_EVENT");
      expect((error as TsqError).message).toContain("empty");
    }
  });

  test("task.updated with non-empty title applies correctly", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-ttl002", { title: "Original", kind: "task", priority: 1 }, 1),
    ]);

    const updated = applyEvent(
      withTask,
      event("task.updated", "tsq-ttl002", { title: "Updated title" }, 2),
    );

    expect(updated.tasks["tsq-ttl002"]?.title).toBe("Updated title");
  });

  test("task.updated without title field preserves existing title", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-ttl003", { title: "Keep me", kind: "task", priority: 1 }, 1),
    ]);

    const updated = applyEvent(withTask, event("task.updated", "tsq-ttl003", { priority: 2 }, 2));

    expect(updated.tasks["tsq-ttl003"]?.title).toBe("Keep me");
  });
});

describe("projector task.status_set transitions", () => {
  test("task.status_set rejects closed to in_progress transition", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-cls001", { title: "Closed task", kind: "task", priority: 1 }, 1),
      event("task.status_set", "tsq-cls001", { status: "closed" }, 2),
    ]);

    try {
      applyEvent(withTask, event("task.status_set", "tsq-cls001", { status: "in_progress" }, 3));
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("INVALID_TRANSITION");
    }
  });

  test("task.status_set allows closed to open (reopen path)", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-cls002", { title: "Closed task", kind: "task", priority: 1 }, 1),
      event("task.status_set", "tsq-cls002", { status: "closed" }, 2),
    ]);

    const reopened = applyEvent(
      withTask,
      event("task.status_set", "tsq-cls002", { status: "open" }, 3),
    );

    expect(reopened.tasks["tsq-cls002"]?.status).toBe("open");
    expect(reopened.tasks["tsq-cls002"]?.closed_at).toBeUndefined();
  });

  test("task.status_set allows open to in_progress", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-cls003", { title: "Open task", kind: "task", priority: 1 }, 1),
    ]);

    const updated = applyEvent(
      withTask,
      event("task.status_set", "tsq-cls003", { status: "in_progress" }, 2),
    );

    expect(updated.tasks["tsq-cls003"]?.status).toBe("in_progress");
  });

  test("task.status_set rejects canceled to in_progress transition", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-cls004", { title: "Canceled task", kind: "task", priority: 1 }, 1),
      event("task.status_set", "tsq-cls004", { status: "canceled" }, 2),
    ]);

    try {
      applyEvent(withTask, event("task.status_set", "tsq-cls004", { status: "in_progress" }, 3));
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("INVALID_TRANSITION");
    }
  });

  test("task.status_set sets closed_at when transitioning to closed", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-cls005", { title: "To close", kind: "task", priority: 1 }, 1),
    ]);

    const closed = applyEvent(
      withTask,
      event("task.status_set", "tsq-cls005", { status: "closed" }, 2),
    );

    expect(closed.tasks["tsq-cls005"]?.status).toBe("closed");
    expect(closed.tasks["tsq-cls005"]?.closed_at).toBe(at(2));
    expect(closed.tasks["tsq-cls005"]?.updated_at).toBe(at(2));
  });

  test("task.status_set clears closed_at when reopening", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-cls006", { title: "Reopen me", kind: "task", priority: 1 }, 1),
      event("task.status_set", "tsq-cls006", { status: "closed" }, 2),
    ]);

    expect(withTask.tasks["tsq-cls006"]?.closed_at).toBe(at(2));

    const reopened = applyEvent(
      withTask,
      event("task.status_set", "tsq-cls006", { status: "open" }, 3),
    );

    expect(reopened.tasks["tsq-cls006"]?.status).toBe("open");
    expect(reopened.tasks["tsq-cls006"]?.closed_at).toBeUndefined();
  });

  test("task.status_set rejects missing status", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-cls007", { title: "No status", kind: "task", priority: 1 }, 1),
    ]);

    try {
      applyEvent(withTask, event("task.status_set", "tsq-cls007", {}, 2));
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("INVALID_EVENT");
    }
  });

  test("task.status_set rejects invalid status value", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-cls008", { title: "Bad status", kind: "task", priority: 1 }, 1),
    ]);

    try {
      applyEvent(withTask, event("task.status_set", "tsq-cls008", { status: "invalid" }, 2));
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("INVALID_EVENT");
    }
  });
});

describe("projector supersede canonical field only", () => {
  test("task.superseded accepts canonical 'with' field", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-sup001", { title: "Old", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-sup002", { title: "New", kind: "task", priority: 1 }, 2),
    ]);

    const result = applyEvent(
      withTasks,
      event("task.superseded", "tsq-sup001", { with: "tsq-sup002" }, 3),
    );

    expect(result.tasks["tsq-sup001"]?.status).toBe("closed");
    expect(result.tasks["tsq-sup001"]?.superseded_by).toBe("tsq-sup002");
  });

  test("task.superseded rejects payload with only 'new_id' fallback", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-sup003", { title: "Old", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-sup004", { title: "New", kind: "task", priority: 1 }, 2),
    ]);

    try {
      applyEvent(withTasks, event("task.superseded", "tsq-sup003", { new_id: "tsq-sup004" }, 3));
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("INVALID_EVENT");
    }
  });

  test("task.superseded rejects payload with only 'target' fallback", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-sup005", { title: "Old", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-sup006", { title: "New", kind: "task", priority: 1 }, 2),
    ]);

    try {
      applyEvent(withTasks, event("task.superseded", "tsq-sup005", { target: "tsq-sup006" }, 3));
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("INVALID_EVENT");
    }
  });

  test("task.superseded rejects empty payload", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-sup007", { title: "Old", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-sup008", { title: "New", kind: "task", priority: 1 }, 2),
    ]);

    try {
      applyEvent(withTasks, event("task.superseded", "tsq-sup007", {}, 3));
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("INVALID_EVENT");
    }
  });
});

describe("projector link target canonical field only", () => {
  test("link.added accepts canonical 'target' field", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-rel001", { title: "A", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-rel002", { title: "B", kind: "task", priority: 1 }, 2),
    ]);

    const result = applyEvent(
      withTasks,
      event("link.added", "tsq-rel001", { target: "tsq-rel002", type: "relates_to" }, 3),
    );

    expect(result.links["tsq-rel001"]?.relates_to).toEqual(["tsq-rel002"]);
  });

  test("link.added rejects payload with only 'dst' fallback", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-rel003", { title: "A", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-rel004", { title: "B", kind: "task", priority: 1 }, 2),
    ]);

    try {
      applyEvent(
        withTasks,
        event("link.added", "tsq-rel003", { dst: "tsq-rel004", type: "relates_to" }, 3),
      );
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("INVALID_EVENT");
    }
  });

  test("link.added rejects payload with only 'to' fallback", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-rel005", { title: "A", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-rel006", { title: "B", kind: "task", priority: 1 }, 2),
    ]);

    try {
      applyEvent(
        withTasks,
        event("link.added", "tsq-rel005", { to: "tsq-rel006", type: "relates_to" }, 3),
      );
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("INVALID_EVENT");
    }
  });

  test("link.removed rejects payload with only 'dst' fallback", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-rel007", { title: "A", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-rel008", { title: "B", kind: "task", priority: 1 }, 2),
      event("link.added", "tsq-rel007", { target: "tsq-rel008", type: "relates_to" }, 3),
    ]);

    try {
      applyEvent(
        withTasks,
        event("link.removed", "tsq-rel007", { dst: "tsq-rel008", type: "relates_to" }, 4),
      );
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("INVALID_EVENT");
    }
  });
});

describe("projector discovered_from and typed deps", () => {
  test("task.created and task.updated persist discovered_from", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-src001", { title: "Source", kind: "task", priority: 1 }, 1),
      event(
        "task.created",
        "tsq-dst001",
        { title: "Derived", kind: "task", priority: 1, discovered_from: "tsq-src001" },
        2,
      ),
    ]);
    expect(withTasks.tasks["tsq-dst001"]?.discovered_from).toBe("tsq-src001");

    const updated = applyEvent(
      withTasks,
      event("task.updated", "tsq-dst001", { clear_discovered_from: true }, 3),
    );
    expect(updated.tasks["tsq-dst001"]?.discovered_from).toBeUndefined();
  });

  test("task.updated discovered_from requires existing target", () => {
    const withTask = applyEvents(createEmptyState(), [
      event("task.created", "tsq-task01", { title: "Task", kind: "task", priority: 1 }, 1),
    ]);
    try {
      applyEvent(
        withTask,
        event("task.updated", "tsq-task01", { discovered_from: "tsq-missing" }, 2),
      );
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("TASK_NOT_FOUND");
    }
  });

  test("dep.added with starts_after does not enforce cycle checks", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-depa", { title: "A", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-depb", { title: "B", kind: "task", priority: 1 }, 2),
      event("dep.added", "tsq-depa", { blocker: "tsq-depb", dep_type: "starts_after" }, 3),
    ]);
    const cycled = applyEvent(
      withTasks,
      event("dep.added", "tsq-depb", { blocker: "tsq-depa", dep_type: "starts_after" }, 4),
    );
    expect(cycled.deps["tsq-depb"]).toEqual([{ blocker: "tsq-depa", dep_type: "starts_after" }]);
  });

  test("dep.removed removes only matching dependency type", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event("task.created", "tsq-depc", { title: "C", kind: "task", priority: 1 }, 1),
      event("task.created", "tsq-depd", { title: "D", kind: "task", priority: 1 }, 2),
      event("dep.added", "tsq-depc", { blocker: "tsq-depd", dep_type: "starts_after" }, 3),
      event("dep.added", "tsq-depc", { blocker: "tsq-depd", dep_type: "blocks" }, 4),
    ]);

    const removedBlocks = applyEvent(
      withTasks,
      event("dep.removed", "tsq-depc", { blocker: "tsq-depd", dep_type: "blocks" }, 5),
    );
    expect(removedBlocks.deps["tsq-depc"]).toEqual([
      { blocker: "tsq-depd", dep_type: "starts_after" },
    ]);
  });
});
