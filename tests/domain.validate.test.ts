import { describe, expect, test } from "bun:test";

import { assertNoDependencyCycle, isReady, listReady } from "../src/domain/validate";
import { TsqError } from "../src/errors";
import type { DependencyEdge, State, Task, TaskStatus } from "../src/types";

const makeTask = (id: string, status: TaskStatus = "open"): Task => ({
  id,
  kind: "task",
  title: id,
  notes: [],
  status,
  priority: 1,
  labels: [],
  created_at: "2026-02-17T00:00:00.000Z",
  updated_at: "2026-02-17T00:00:00.000Z",
});

const makeState = (
  tasks: Task[],
  deps: Record<string, Array<string | DependencyEdge>> = {},
): State => {
  const byId: Record<string, Task> = {};
  const order: string[] = [];
  for (const task of tasks) {
    byId[task.id] = task;
    order.push(task.id);
  }
  return {
    tasks: byId,
    deps: Object.fromEntries(
      Object.entries(deps).map(([child, edges]) => [
        child,
        edges.map((edge) =>
          typeof edge === "string" ? { blocker: edge, dep_type: "blocks" as const } : edge,
        ),
      ]),
    ),
    links: {},
    child_counters: {},
    created_order: order,
    applied_events: 0,
  };
};

describe("assertNoDependencyCycle", () => {
  test("throws when adding edge creates dependency cycle", () => {
    const state = makeState([makeTask("a"), makeTask("b"), makeTask("c")], {
      a: ["b"],
      b: ["c"],
    });

    try {
      assertNoDependencyCycle(state, "c", "a");
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("DEPENDENCY_CYCLE");
    }
  });

  test("throws on self dependency", () => {
    const state = makeState([makeTask("a")]);

    try {
      assertNoDependencyCycle(state, "a", "a");
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("DEPENDENCY_CYCLE");
    }
  });
});

describe("ready semantics", () => {
  test("considers blocker status and treats missing blockers as blocking", () => {
    const state = makeState(
      [
        makeTask("a", "open"),
        makeTask("b", "in_progress"),
        makeTask("c", "closed"),
        makeTask("d", "canceled"),
        makeTask("e", "blocked"),
        makeTask("f", "open"),
        makeTask("g", "open"),
      ],
      {
        a: ["c"],
        b: ["d"],
        f: ["e"],
        g: ["missing"],
      },
    );

    expect(isReady(state, "a")).toBe(true);
    expect(isReady(state, "b")).toBe(true);
    expect(isReady(state, "f")).toBe(false);
    expect(isReady(state, "g")).toBe(false);
  });

  test("lists only ready tasks in created order", () => {
    const state = makeState(
      [makeTask("first", "open"), makeTask("second", "blocked"), makeTask("third", "in_progress")],
      {
        third: ["second"],
      },
    );

    const ready = listReady(state);

    expect(ready.map((task) => task.id)).toEqual(["first"]);
  });

  test("starts_after dependencies do not block readiness", () => {
    const state = makeState([makeTask("a", "open"), makeTask("b", "open")], {
      a: [{ blocker: "b", dep_type: "starts_after" }],
    });

    expect(isReady(state, "a")).toBe(true);
  });
});
