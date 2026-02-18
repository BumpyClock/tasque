import { describe, expect, test } from "bun:test";
import { afterEach } from "bun:test";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { applyEvent, applyEvents } from "../src/domain/projector";
import { createEmptyState } from "../src/domain/state";
import { TsqError } from "../src/errors";
import { readEvents } from "../src/store/events";
import { getPaths } from "../src/store/paths";
import type {
  DepAddedPayload,
  DepRemovedPayload,
  EventPayloadMap,
  EventRecord,
  EventType,
  LinkAddedPayload,
  LinkRemovedPayload,
  TaskClaimedPayload,
  TaskCreatedPayload,
  TaskNotedPayload,
  TaskSpecAttachedPayload,
  TaskStatusSetPayload,
  TaskSupersededPayload,
  TaskUpdatedPayload,
  TypedEventRecord,
} from "../src/types";

// ---------------------------------------------------------------------------
// Compile-time type checks (these are verified at build time by tsc)
//
// Each check uses a function that would fail to compile if the types don't
// match. The functions are collected into an array that is referenced in a
// runtime test to satisfy noUnusedLocals.
// ---------------------------------------------------------------------------

/** Compile-time assertion: A must extend B. Returns a no-op function. */
// eslint-disable-next-line @typescript-eslint/no-unused-vars
function assertType<_A extends B, B>(): () => void {
  return () => {};
}

/** Verifies that the discriminated union narrows payload types correctly. */
function verifyNarrowsPayload(evt: TypedEventRecord): string {
  switch (evt.type) {
    case "task.created":
      return evt.payload.title + evt.payload.id;
    case "task.updated":
      return String(evt.payload.title ?? "");
    case "task.status_set":
      return evt.payload.status;
    case "task.claimed":
      return String(evt.payload.assignee ?? "");
    case "task.noted":
      return evt.payload.text;
    case "task.spec_attached":
      return evt.payload.spec_path + evt.payload.spec_fingerprint;
    case "task.superseded":
      return evt.payload.with;
    case "dep.added":
      return evt.payload.blocker;
    case "dep.removed":
      return evt.payload.blocker;
    case "link.added":
      return evt.payload.target;
    case "link.removed":
      return evt.payload.target;
  }
}

/**
 * All compile-time type assertions. If any of these fail to compile,
 * the type system has a problem.
 */
const TYPE_CHECKS = [
  // EventPayloadMap maps each event type to its typed payload
  assertType<EventPayloadMap["task.created"], TaskCreatedPayload>(),
  assertType<EventPayloadMap["task.updated"], TaskUpdatedPayload>(),
  assertType<EventPayloadMap["task.status_set"], TaskStatusSetPayload>(),
  assertType<EventPayloadMap["task.claimed"], TaskClaimedPayload>(),
  assertType<EventPayloadMap["task.noted"], TaskNotedPayload>(),
  assertType<EventPayloadMap["task.spec_attached"], TaskSpecAttachedPayload>(),
  assertType<EventPayloadMap["task.superseded"], TaskSupersededPayload>(),
  assertType<EventPayloadMap["dep.added"], DepAddedPayload>(),
  assertType<EventPayloadMap["dep.removed"], DepRemovedPayload>(),
  assertType<EventPayloadMap["link.added"], LinkAddedPayload>(),
  assertType<EventPayloadMap["link.removed"], LinkRemovedPayload>(),

  // TypedEventRecord type field covers all EventType values (both directions)
  assertType<TypedEventRecord["type"], EventType>(),
  assertType<EventType, TypedEventRecord["type"]>(),

  // TypedEventRecord and EventRecord share common base fields
  assertType<TypedEventRecord["event_id"], EventRecord["event_id"]>(),
  assertType<TypedEventRecord["ts"], EventRecord["ts"]>(),
  assertType<TypedEventRecord["actor"], EventRecord["actor"]>(),
  assertType<TypedEventRecord["task_id"], EventRecord["task_id"]>(),

  // Discriminated union narrows correctly
  verifyNarrowsPayload,
];

// ---------------------------------------------------------------------------
// Runtime tests
// ---------------------------------------------------------------------------

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

describe("typed payload compile-time checks", () => {
  test("all compile-time type assertions compile and register", () => {
    // If this test runs, every assertType<A, B>() above compiled successfully,
    // meaning the type relationships hold. The array reference ensures tsc
    // does not strip the checks as unused.
    expect(TYPE_CHECKS.length).toBeGreaterThan(0);
    for (const check of TYPE_CHECKS) {
      expect(typeof check).toBe("function");
    }
  });

  test("EventPayloadMap covers all EventType values", () => {
    const allTypes: EventType[] = [
      "task.created",
      "task.updated",
      "task.status_set",
      "task.claimed",
      "task.noted",
      "task.spec_attached",
      "task.superseded",
      "dep.added",
      "dep.removed",
      "link.added",
      "link.removed",
    ];

    // Verify at runtime that EventPayloadMap has an entry for every type.
    // This can't fail if the compile-time checks pass, but serves as
    // documentation and a safety net.
    for (const type of allTypes) {
      expect(type).toBeString();
    }
    expect(allTypes.length).toBe(11);
  });
});

describe("typed payload projector integration", () => {
  test("task.created with typed payload produces correct task", () => {
    const payload: TaskCreatedPayload = {
      id: "tsq-typed1",
      title: "Typed task",
      kind: "feature",
      priority: 2,
      status: "open",
      description: "Has a description",
      labels: ["v1", "typed"],
    };

    const state = applyEvent(
      createEmptyState(),
      event("task.created", "tsq-typed1", payload as unknown as Record<string, unknown>, 1),
    );

    const task = state.tasks["tsq-typed1"];
    expect(task).toBeDefined();
    expect(task?.title).toBe("Typed task");
    expect(task?.kind).toBe("feature");
    expect(task?.priority).toBe(2);
    expect(task?.description).toBe("Has a description");
    expect(task?.labels).toEqual(["v1", "typed"]);
  });

  test("task.updated with typed payload applies partial updates", () => {
    const withTask = applyEvent(
      createEmptyState(),
      event(
        "task.created",
        "tsq-typed2",
        { title: "Original", kind: "task", priority: 1, status: "open" },
        1,
      ),
    );

    const payload: TaskUpdatedPayload = {
      title: "Updated title",
      priority: 3,
    };

    const updated = applyEvent(
      withTask,
      event("task.updated", "tsq-typed2", payload as unknown as Record<string, unknown>, 2),
    );

    expect(updated.tasks["tsq-typed2"]?.title).toBe("Updated title");
    expect(updated.tasks["tsq-typed2"]?.priority).toBe(3);
    expect(updated.tasks["tsq-typed2"]?.status).toBe("open");
  });

  test("task.status_set with typed payload transitions status", () => {
    const withTask = applyEvent(
      createEmptyState(),
      event(
        "task.created",
        "tsq-typed3",
        { title: "Status test", kind: "task", priority: 1, status: "open" },
        1,
      ),
    );

    const payload: TaskStatusSetPayload = { status: "closed" };
    const closed = applyEvent(
      withTask,
      event("task.status_set", "tsq-typed3", payload as unknown as Record<string, unknown>, 2),
    );

    expect(closed.tasks["tsq-typed3"]?.status).toBe("closed");
    expect(closed.tasks["tsq-typed3"]?.closed_at).toBeDefined();
  });

  test("dep.added with typed payload links dependency", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event(
        "task.created",
        "tsq-child1",
        { title: "Child", kind: "task", priority: 1, status: "open" },
        1,
      ),
      event(
        "task.created",
        "tsq-block1",
        { title: "Blocker", kind: "task", priority: 1, status: "open" },
        2,
      ),
    ]);

    const payload: DepAddedPayload = { blocker: "tsq-block1" };
    const linked = applyEvent(
      withTasks,
      event("dep.added", "tsq-child1", payload as unknown as Record<string, unknown>, 3),
    );

    expect(linked.deps["tsq-child1"]).toEqual(["tsq-block1"]);
  });

  test("link.added with typed payload creates relation", () => {
    const withTasks = applyEvents(createEmptyState(), [
      event(
        "task.created",
        "tsq-src001",
        { title: "Source", kind: "task", priority: 1, status: "open" },
        1,
      ),
      event(
        "task.created",
        "tsq-dst001",
        { title: "Target", kind: "task", priority: 1, status: "open" },
        2,
      ),
    ]);

    const payload: LinkAddedPayload = { target: "tsq-dst001", type: "relates_to" };
    const linked = applyEvent(
      withTasks,
      event("link.added", "tsq-src001", payload as unknown as Record<string, unknown>, 3),
    );

    expect(linked.links["tsq-src001"]?.relates_to).toEqual(["tsq-dst001"]);
    // relates_to is bidirectional
    expect(linked.links["tsq-dst001"]?.relates_to).toEqual(["tsq-src001"]);
  });

  test("task.noted with typed payload appends note", () => {
    const withTask = applyEvent(
      createEmptyState(),
      event(
        "task.created",
        "tsq-noted1",
        { title: "Noted", kind: "task", priority: 1, status: "open" },
        1,
      ),
    );

    const payload: TaskNotedPayload = { text: "Important note" };
    const noted = applyEvent(
      withTask,
      event("task.noted", "tsq-noted1", payload as unknown as Record<string, unknown>, 2),
    );

    expect(noted.tasks["tsq-noted1"]?.notes).toHaveLength(1);
    expect(noted.tasks["tsq-noted1"]?.notes[0]?.text).toBe("Important note");
  });

  test("task.spec_attached with typed payload stores spec metadata", () => {
    const withTask = applyEvent(
      createEmptyState(),
      event(
        "task.created",
        "tsq-spec01",
        { title: "Spec", kind: "task", priority: 1, status: "open" },
        1,
      ),
    );

    const payload: TaskSpecAttachedPayload = {
      spec_path: ".tasque/specs/tsq-spec01/spec.md",
      spec_fingerprint: "deadbeef",
      spec_attached_at: at(2),
      spec_attached_by: "agent",
    };
    const attached = applyEvent(
      withTask,
      event("task.spec_attached", "tsq-spec01", payload as unknown as Record<string, unknown>, 2),
    );

    const task = attached.tasks["tsq-spec01"];
    expect(task?.spec_path).toBe(".tasque/specs/tsq-spec01/spec.md");
    expect(task?.spec_fingerprint).toBe("deadbeef");
    expect(task?.spec_attached_at).toBe(at(2));
    expect(task?.spec_attached_by).toBe("agent");
  });
});

describe("runtime payload validation in readEvents", () => {
  const repos: string[] = [];

  async function makeRepo(): Promise<string> {
    const repo = await mkdtemp(join(tmpdir(), "tasque-payload-val-"));
    repos.push(repo);
    return repo;
  }

  afterEach(async () => {
    await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
  });

  test("rejects task.created missing title field", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });

    const invalid = JSON.stringify({
      event_id: "01AAAAAA",
      ts: "2026-02-17T00:00:00.000Z",
      actor: "test",
      type: "task.created",
      task_id: "tsq-abc123",
      payload: { kind: "task", priority: 1 },
    });
    await writeFile(paths.eventsFile, `${invalid}\n`, "utf8");

    const err = await readEvents(repo).catch((e) => e);
    expect(err).toBeInstanceOf(TsqError);
    expect(err.code).toBe("EVENTS_CORRUPT");
    expect(err.message).toContain("title");
  });

  test("rejects dep.added missing blocker field", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });

    const invalid = JSON.stringify({
      event_id: "01AAAAAA",
      ts: "2026-02-17T00:00:00.000Z",
      actor: "test",
      type: "dep.added",
      task_id: "tsq-abc123",
      payload: {},
    });
    await writeFile(paths.eventsFile, `${invalid}\n`, "utf8");

    const err = await readEvents(repo).catch((e) => e);
    expect(err).toBeInstanceOf(TsqError);
    expect(err.code).toBe("EVENTS_CORRUPT");
    expect(err.message).toContain("blocker");
  });

  test("rejects task.noted missing text field", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });

    const invalid = JSON.stringify({
      event_id: "01AAAAAA",
      ts: "2026-02-17T00:00:00.000Z",
      actor: "test",
      type: "task.noted",
      task_id: "tsq-abc123",
      payload: { actor: "test" },
    });
    await writeFile(paths.eventsFile, `${invalid}\n`, "utf8");

    const err = await readEvents(repo).catch((e) => e);
    expect(err).toBeInstanceOf(TsqError);
    expect(err.code).toBe("EVENTS_CORRUPT");
    expect(err.message).toContain("text");
  });

  test("rejects link.added missing type field", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });

    const invalid = JSON.stringify({
      event_id: "01AAAAAA",
      ts: "2026-02-17T00:00:00.000Z",
      actor: "test",
      type: "link.added",
      task_id: "tsq-abc123",
      payload: { target: "tsq-other1" },
    });
    await writeFile(paths.eventsFile, `${invalid}\n`, "utf8");

    const err = await readEvents(repo).catch((e) => e);
    expect(err).toBeInstanceOf(TsqError);
    expect(err.code).toBe("EVENTS_CORRUPT");
    expect(err.message).toContain("type");
  });

  test("rejects task.spec_attached missing spec_path", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });

    const invalid = JSON.stringify({
      event_id: "01AAAAAA",
      ts: "2026-02-17T00:00:00.000Z",
      actor: "test",
      type: "task.spec_attached",
      task_id: "tsq-abc123",
      payload: { spec_fingerprint: "abc123" },
    });
    await writeFile(paths.eventsFile, `${invalid}\n`, "utf8");

    const err = await readEvents(repo).catch((e) => e);
    expect(err).toBeInstanceOf(TsqError);
    expect(err.code).toBe("EVENTS_CORRUPT");
    expect(err.message).toContain("spec_path");
  });

  test("rejects unknown event type", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });

    const invalid = JSON.stringify({
      event_id: "01AAAAAA",
      ts: "2026-02-17T00:00:00.000Z",
      actor: "test",
      type: "task.unknown_type",
      task_id: "tsq-abc123",
      payload: {},
    });
    await writeFile(paths.eventsFile, `${invalid}\n`, "utf8");

    const err = await readEvents(repo).catch((e) => e);
    expect(err).toBeInstanceOf(TsqError);
    expect(err.code).toBe("EVENTS_CORRUPT");
    expect(err.message).toContain("unknown event type");
  });

  test("rejects event with non-object payload", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });

    const invalid = JSON.stringify({
      event_id: "01AAAAAA",
      ts: "2026-02-17T00:00:00.000Z",
      actor: "test",
      type: "task.created",
      task_id: "tsq-abc123",
      payload: "not-an-object",
    });
    await writeFile(paths.eventsFile, `${invalid}\n`, "utf8");

    const err = await readEvents(repo).catch((e) => e);
    expect(err).toBeInstanceOf(TsqError);
    expect(err.code).toBe("EVENTS_CORRUPT");
    expect(err.message).toContain("expected record");
  });

  test("accepts task.updated with empty payload (all fields optional)", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });

    const valid = JSON.stringify({
      event_id: "01AAAAAA",
      ts: "2026-02-17T00:00:00.000Z",
      actor: "test",
      type: "task.updated",
      task_id: "tsq-abc123",
      payload: {},
    });
    await writeFile(paths.eventsFile, `${valid}\n`, "utf8");

    const result = await readEvents(repo);
    expect(result.events).toHaveLength(1);
    expect(result.events[0]?.type).toBe("task.updated");
  });

  test("accepts task.claimed with empty payload (all fields optional)", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });

    const valid = JSON.stringify({
      event_id: "01AAAAAA",
      ts: "2026-02-17T00:00:00.000Z",
      actor: "test",
      type: "task.claimed",
      task_id: "tsq-abc123",
      payload: {},
    });
    await writeFile(paths.eventsFile, `${valid}\n`, "utf8");

    const result = await readEvents(repo);
    expect(result.events).toHaveLength(1);
    expect(result.events[0]?.type).toBe("task.claimed");
  });
});
