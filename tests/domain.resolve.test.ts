import { describe, expect, test } from "bun:test";

import { resolveTaskId } from "../src/domain/resolve";
import { TsqError } from "../src/errors";
import type { State, Task } from "../src/types";

const makeTask = (id: string): Task => ({
  id,
  kind: "task",
  title: id,
  status: "open",
  priority: 1,
  labels: [],
  created_at: "2026-02-17T00:00:00.000Z",
  updated_at: "2026-02-17T00:00:00.000Z",
});

const makeState = (ids: string[]): State => {
  const tasks: Record<string, Task> = {};
  for (const id of ids) {
    tasks[id] = makeTask(id);
  }
  return {
    tasks,
    deps: {},
    links: {},
    child_counters: {},
    created_order: ids,
    applied_events: 0,
  };
};

describe("resolveTaskId", () => {
  test("returns exact id match even when it is also a partial prefix", () => {
    const state = makeState(["tsq-ab12cd", "tsq-ab12ef"]);

    const resolved = resolveTaskId(state, "tsq-ab12cd");

    expect(resolved).toBe("tsq-ab12cd");
  });

  test("returns unique partial id match", () => {
    const state = makeState(["tsq-ab12cd", "tsq-ef34gh"]);

    const resolved = resolveTaskId(state, "tsq-ab1");

    expect(resolved).toBe("tsq-ab12cd");
  });

  test("throws on ambiguous partial id match", () => {
    const state = makeState(["tsq-ab12cd", "tsq-ab12ef", "tsq-zz99yy"]);

    try {
      resolveTaskId(state, "tsq-ab12");
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("TASK_ID_AMBIGUOUS");
    }
  });

  test("throws when exact flag is used with non-exact id", () => {
    const state = makeState(["tsq-ab12cd"]);

    try {
      resolveTaskId(state, "tsq-ab1", true);
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(TsqError);
      expect((error as TsqError).code).toBe("TASK_NOT_FOUND");
    }
  });
});
