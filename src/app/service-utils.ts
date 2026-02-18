import { ulid } from "ulid";
import { makeRootId } from "../domain/ids";
import { resolveTaskId } from "../domain/resolve";
import { TsqError } from "../errors";
import type { State, Task, TaskStatus } from "../types";
import type { ListFilter } from "./service-types";

export const DEFAULT_STALE_STATUSES: TaskStatus[] = ["open", "in_progress", "blocked", "deferred"];

export function uniqueRootId(state: State, title: string): string {
  const maxAttempts = 10;
  for (let idx = 0; idx < maxAttempts; idx += 1) {
    const nonce = idx === 0 ? undefined : ulid();
    const id = makeRootId(title, nonce);
    if (!state.tasks[id]) {
      return id;
    }
  }
  throw new TsqError("ID_COLLISION", "unable to allocate unique task id", 2);
}

export function mustTask(state: State, id: string): Task {
  const task = state.tasks[id];
  if (!task) {
    throw new TsqError("NOT_FOUND", `task not found: ${id}`, 1);
  }
  return task;
}

export function mustResolveExisting(state: State, raw: string, exactId?: boolean): string {
  const id = resolveTaskId(state, raw, exactId);
  if (!state.tasks[id]) {
    throw new TsqError("NOT_FOUND", `task not found: ${raw}`, 1);
  }
  return id;
}

export function sortTasks(tasks: Task[]): Task[] {
  return [...tasks].sort((a, b) => {
    if (a.priority !== b.priority) {
      return a.priority - b.priority;
    }
    if (a.created_at === b.created_at) {
      return a.id.localeCompare(b.id);
    }
    return a.created_at.localeCompare(b.created_at);
  });
}

export function sortStaleTasks(tasks: Task[]): Task[] {
  return [...tasks].sort((a, b) => {
    if (a.updated_at !== b.updated_at) {
      return a.updated_at.localeCompare(b.updated_at);
    }
    if (a.priority !== b.priority) {
      return a.priority - b.priority;
    }
    return a.id.localeCompare(b.id);
  });
}

export function applyListFilter(tasks: Task[], filter: ListFilter): Task[] {
  return tasks.filter((task) => {
    if (filter.statuses && !filter.statuses.includes(task.status)) {
      return false;
    }
    if (filter.ids && !filter.ids.includes(task.id)) {
      return false;
    }
    if (filter.assignee && task.assignee !== filter.assignee) {
      return false;
    }
    if (filter.externalRef && task.external_ref !== filter.externalRef) {
      return false;
    }
    if (filter.discoveredFrom && task.discovered_from !== filter.discoveredFrom) {
      return false;
    }
    if (filter.unassigned && hasAssignee(task.assignee)) {
      return false;
    }
    if (filter.kind && task.kind !== filter.kind) {
      return false;
    }
    if (filter.label && !task.labels.includes(filter.label)) {
      return false;
    }
    if (filter.labelAny && !filter.labelAny.some((label) => task.labels.includes(label))) {
      return false;
    }
    if (filter.createdAfter && task.created_at <= filter.createdAfter) {
      return false;
    }
    if (filter.updatedAfter && task.updated_at <= filter.updatedAfter) {
      return false;
    }
    if (filter.closedAfter) {
      if (!task.closed_at) {
        return false;
      }
      if (task.closed_at <= filter.closedAfter) {
        return false;
      }
    }
    if (filter.planning_state && task.planning_state !== filter.planning_state) {
      return false;
    }
    return true;
  });
}

function hasAssignee(value: string | undefined): boolean {
  return typeof value === "string" && value.trim().length > 0;
}

export function hasDuplicateLink(state: State, source: string, canonical: string): boolean {
  return (state.links[source]?.duplicates ?? []).includes(canonical);
}

export function createsDuplicateCycle(state: State, source: string, canonical: string): boolean {
  const visited = new Set<string>();
  let cursor: string | undefined = canonical;
  while (cursor) {
    if (cursor === source) {
      return true;
    }
    if (visited.has(cursor)) {
      return false;
    }
    visited.add(cursor);
    cursor = state.tasks[cursor]?.duplicate_of;
  }
  return false;
}

export function normalizeDuplicateTitle(title: string): string {
  return title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

export function sortTaskIds(taskIds: string[]): string[] {
  return [...taskIds].sort((a, b) => a.localeCompare(b));
}
