import type { BoardLane, TaskRecord, TaskStatus } from "./types";

export interface BoardColumns {
  open: TaskRecord[];
  in_progress: TaskRecord[];
  done: TaskRecord[];
}

const DONE_STATUSES: TaskStatus[] = ["closed", "canceled"];

export function toBoardColumns(tasks: TaskRecord[]): BoardColumns {
  const columns: BoardColumns = {
    open: [],
    in_progress: [],
    done: [],
  };

  for (const task of tasks) {
    if (task.status === "open" || task.status === "deferred") {
      columns.open.push(task);
      continue;
    }
    if (task.status === "in_progress" || task.status === "blocked") {
      columns.in_progress.push(task);
      continue;
    }
    columns.done.push(task);
  }

  return columns;
}

export function toEpicChildren(tasks: TaskRecord[], epicId: string): TaskRecord[] {
  return tasks.filter((task) => task.parent_id === epicId);
}

export function epicProgress(tasks: TaskRecord[], epicId: string): {
  done: number;
  total: number;
  open: number;
  inProgress: number;
} {
  const children = toEpicChildren(tasks, epicId);

  let done = 0;
  let open = 0;
  let inProgress = 0;
  for (const child of children) {
    if (DONE_STATUSES.includes(child.status)) {
      done += 1;
      continue;
    }
    if (child.status === "in_progress" || child.status === "blocked") {
      inProgress += 1;
      continue;
    }
    open += 1;
  }

  return {
    done,
    total: children.length,
    open,
    inProgress,
  };
}

export function laneOrder(): BoardLane[] {
  return ["open", "in_progress", "done"];
}

export function nextTab<T>(items: T[], current: T, direction: 1 | -1): T {
  if (items.length === 0) {
    return current;
  }
  const index = items.findIndex((item) => item === current);
  const safeIndex = index >= 0 ? index : 0;
  const next = (safeIndex + direction + items.length) % items.length;
  return items[next] as T;
}

export function nextItemId(items: TaskRecord[], currentId: string | null, direction: 1 | -1): string | null {
  if (items.length === 0) {
    return null;
  }

  const index = currentId ? items.findIndex((item) => item.id === currentId) : -1;
  const base = index >= 0 ? index : 0;
  const next = Math.max(0, Math.min(items.length - 1, base + direction));
  return items[next]?.id ?? items[0]!.id;
}

export function clampSelectionId(items: TaskRecord[], currentId: string | null): string | null {
  if (items.length === 0) {
    return null;
  }
  if (currentId && items.some((item) => item.id === currentId)) {
    return currentId;
  }
  return items[0]!.id;
}

export function truncate(value: string, maxChars: number): string {
  if (maxChars <= 0) {
    return "";
  }
  if (value.length <= maxChars) {
    return value;
  }
  if (maxChars <= 1) {
    return value.slice(0, maxChars);
  }
  return `${value.slice(0, maxChars - 1)}~`;
}

export function pad(value: string, width: number): string {
  if (width <= 0) {
    return "";
  }
  if (value.length >= width) {
    return truncate(value, width);
  }
  return `${value}${" ".repeat(width - value.length)}`;
}
