import { TsqError } from "../errors";
import type { DependencyType, State, Task, TaskStatus } from "../types";
import { normalizeDependencyEdges } from "./deps";

export type PlanningLane = "planning" | "coding";

const OPEN_READY_STATUSES = new Set(["open", "in_progress"] as const);
const CLOSED_BLOCKER_STATUSES = new Set(["closed", "canceled"] as const);
const isOpenReadyStatus = (status: TaskStatus): status is "open" | "in_progress" =>
  OPEN_READY_STATUSES.has(status as "open" | "in_progress");
const isClosedBlockerStatus = (status: TaskStatus): status is "closed" | "canceled" =>
  CLOSED_BLOCKER_STATUSES.has(status as "closed" | "canceled");
const BLOCKS_DEP_TYPE: DependencyType = "blocks";

function blockingDepIds(state: State, taskId: string): string[] {
  return normalizeDependencyEdges(state.deps[taskId] as unknown)
    .filter((edge) => edge.dep_type === BLOCKS_DEP_TYPE)
    .map((edge) => edge.blocker);
}

export const assertNoDependencyCycle = (state: State, child: string, blocker: string): void => {
  if (child === blocker) {
    throw new TsqError("DEPENDENCY_CYCLE", "Dependency cycle detected", 1, { child, blocker });
  }

  const stack = [blocker];
  const visited = new Set<string>();
  while (stack.length > 0) {
    const current = stack.pop();
    if (!current || visited.has(current)) {
      continue;
    }
    if (current === child) {
      throw new TsqError("DEPENDENCY_CYCLE", "Dependency cycle detected", 1, { child, blocker });
    }
    visited.add(current);
    for (const next of blockingDepIds(state, current)) {
      if (!visited.has(next)) {
        stack.push(next);
      }
    }
  }
};

export const isReady = (state: State, taskId: string): boolean => {
  const task = state.tasks[taskId];
  if (!task) {
    return false;
  }
  if (!isOpenReadyStatus(task.status)) {
    return false;
  }

  for (const blockerId of blockingDepIds(state, taskId)) {
    const blocker = state.tasks[blockerId];
    if (!blocker) {
      return false;
    }
    if (!isClosedBlockerStatus(blocker.status)) {
      return false;
    }
  }
  return true;
};

export const listReady = (state: State): Task[] => {
  const ready: Task[] = [];

  for (const id of state.created_order) {
    const task = state.tasks[id];
    if (!task) {
      continue;
    }
    if (isReady(state, id)) {
      ready.push(task);
    }
  }

  return ready;
};

export const listReadyByLane = (state: State, lane?: PlanningLane): Task[] => {
  const all = listReady(state);
  if (!lane) {
    return all;
  }
  if (lane === "planning") {
    return all.filter((t) => !t.planning_state || t.planning_state === "needs_planning");
  }
  return all.filter((t) => t.planning_state === "planned");
};
