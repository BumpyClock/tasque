import { TsqError } from "../errors";
import type { State, Task, TaskStatus } from "../types";

const OPEN_READY_STATUSES = new Set(["open", "in_progress"] as const);
const CLOSED_BLOCKER_STATUSES = new Set(["closed", "canceled"] as const);
const isOpenReadyStatus = (status: TaskStatus): status is "open" | "in_progress" =>
  OPEN_READY_STATUSES.has(status as "open" | "in_progress");
const isClosedBlockerStatus = (status: TaskStatus): status is "closed" | "canceled" =>
  CLOSED_BLOCKER_STATUSES.has(status as "closed" | "canceled");

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
    for (const next of state.deps[current] ?? []) {
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

  for (const blockerId of state.deps[taskId] ?? []) {
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
