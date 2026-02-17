import { TsqError } from "../errors";
import type {
  EventRecord,
  Priority,
  RelationType,
  State,
  Task,
  TaskKind,
  TaskStatus,
} from "../types";

const RELATION_TYPES: RelationType[] = ["relates_to", "replies_to", "duplicates", "supersedes"];
const TASK_KINDS: TaskKind[] = ["task", "feature", "epic"];
const TASK_STATUSES: TaskStatus[] = ["open", "in_progress", "blocked", "closed", "canceled"];

const asObject = (value: unknown): Record<string, unknown> => {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return {};
  }
  return value as Record<string, unknown>;
};

const asString = (value: unknown): string | undefined => {
  return typeof value === "string" ? value : undefined;
};

const asStringArray = (value: unknown): string[] | undefined => {
  if (!Array.isArray(value)) {
    return undefined;
  }
  const strings = value.filter((entry): entry is string => typeof entry === "string");
  return strings.length === value.length ? strings : undefined;
};

const asPriority = (value: unknown): Priority | undefined => {
  if (typeof value !== "number") {
    return undefined;
  }
  if (!Number.isInteger(value) || value < 0 || value > 3) {
    return undefined;
  }
  return value as Priority;
};

const asTaskKind = (value: unknown): TaskKind | undefined => {
  return typeof value === "string" && TASK_KINDS.includes(value as TaskKind)
    ? (value as TaskKind)
    : undefined;
};

const asTaskStatus = (value: unknown): TaskStatus | undefined => {
  return typeof value === "string" && TASK_STATUSES.includes(value as TaskStatus)
    ? (value as TaskStatus)
    : undefined;
};

const asRelationType = (value: unknown): RelationType | undefined => {
  return typeof value === "string" && RELATION_TYPES.includes(value as RelationType)
    ? (value as RelationType)
    : undefined;
};

const cloneState = (state: State): State => ({
  tasks: { ...state.tasks },
  deps: Object.fromEntries(Object.entries(state.deps).map(([id, deps]) => [id, [...deps]])),
  links: Object.fromEntries(
    Object.entries(state.links).map(([id, rels]) => [
      id,
      Object.fromEntries(
        Object.entries(rels).map(([type, targets]) => [type, targets ? [...targets] : []]),
      ),
    ]),
  ),
  child_counters: { ...state.child_counters },
  created_order: [...state.created_order],
  applied_events: state.applied_events,
});

const setChildCounter = (state: State, parentId: string, childId: string): void => {
  const prefix = `${parentId}.`;
  if (!childId.startsWith(prefix)) {
    return;
  }
  const segment = childId.slice(prefix.length);
  if (!/^\d+$/.test(segment)) {
    return;
  }
  const counter = Number.parseInt(segment, 10);
  const current = state.child_counters[parentId] ?? 0;
  if (counter > current) {
    state.child_counters[parentId] = counter;
  }
};

const setTaskClosedState = (task: Task, ts: string): Task => ({
  ...task,
  status: "closed",
  updated_at: ts,
  closed_at: ts,
});

const upsertDirectedLink = (
  links: State["links"],
  src: string,
  dst: string,
  type: RelationType,
): void => {
  if (!links[src]) {
    links[src] = {};
  }
  const current = links[src][type] ?? [];
  if (!current.includes(dst)) {
    links[src][type] = [...current, dst];
  }
};

const removeDirectedLink = (
  links: State["links"],
  src: string,
  dst: string,
  type: RelationType,
): void => {
  const from = links[src];
  if (!from) {
    return;
  }
  const current = from[type];
  if (!current) {
    return;
  }
  from[type] = current.filter((candidate) => candidate !== dst);
};

const requireTask = (state: State, taskId: string): Task => {
  const task = state.tasks[taskId];
  if (!task) {
    throw new TsqError("TASK_NOT_FOUND", "Task not found", 1, { task_id: taskId });
  }
  return task;
};

const applyTaskCreated = (state: State, event: EventRecord): void => {
  if (state.tasks[event.task_id]) {
    throw new TsqError("TASK_EXISTS", "Task already exists", 1, { task_id: event.task_id });
  }
  const payload = asObject(event.payload);
  const title = asString(payload.title);
  if (!title || title.length === 0) {
    throw new TsqError("INVALID_EVENT", "task.created requires a title", 1, {
      event_id: event.event_id,
    });
  }

  const kind = asTaskKind(payload.kind) ?? "task";
  const priority = asPriority(payload.priority) ?? 1;
  const status = asTaskStatus(payload.status) ?? "open";
  const labels = asStringArray(payload.labels) ?? [];
  const parentId = asString(payload.parent_id);
  const task: Task = {
    id: event.task_id,
    title,
    kind,
    status,
    priority,
    assignee: asString(payload.assignee),
    parent_id: parentId,
    superseded_by: asString(payload.superseded_by),
    duplicate_of: asString(payload.duplicate_of),
    replies_to: asString(payload.replies_to),
    labels,
    created_at: event.ts,
    updated_at: event.ts,
    closed_at: status === "closed" ? event.ts : undefined,
  };
  state.tasks[event.task_id] = task;
  state.created_order.push(event.task_id);
  if (parentId) {
    setChildCounter(state, parentId, task.id);
  }
};

const applyTaskUpdated = (state: State, event: EventRecord): void => {
  const current = requireTask(state, event.task_id);
  const payload = asObject(event.payload);
  const next: Task = {
    ...current,
    updated_at: event.ts,
  };

  const title = asString(payload.title);
  if (title) {
    next.title = title;
  }
  const kind = asTaskKind(payload.kind);
  if (kind) {
    next.kind = kind;
  }
  const status = asTaskStatus(payload.status);
  if (status) {
    next.status = status;
  }
  const priority = asPriority(payload.priority);
  if (priority !== undefined) {
    next.priority = priority;
  }
  const assignee = asString(payload.assignee);
  if (assignee !== undefined) {
    next.assignee = assignee;
  }
  const labels = asStringArray(payload.labels);
  if (labels) {
    next.labels = labels;
  }

  if (next.status === "closed") {
    next.closed_at = next.closed_at ?? event.ts;
  } else {
    next.closed_at = undefined;
  }
  state.tasks[event.task_id] = next;
};

const applyTaskClaimed = (state: State, event: EventRecord): void => {
  const current = requireTask(state, event.task_id);
  const payload = asObject(event.payload);
  const assignee = asString(payload.assignee) ?? event.actor;
  state.tasks[event.task_id] = {
    ...current,
    assignee,
    updated_at: event.ts,
  };
};

const applyTaskSuperseded = (state: State, event: EventRecord): void => {
  const source = requireTask(state, event.task_id);
  const payload = asObject(event.payload);
  const replacement =
    asString(payload.with) ?? asString(payload.new_id) ?? asString(payload.target);
  if (!replacement) {
    throw new TsqError("INVALID_EVENT", "task.superseded requires replacement task", 1, {
      event_id: event.event_id,
    });
  }
  if (replacement === event.task_id) {
    throw new TsqError("INVALID_EVENT", "Task cannot supersede itself", 1, {
      task_id: event.task_id,
    });
  }
  requireTask(state, replacement);
  state.tasks[event.task_id] = {
    ...setTaskClosedState(source, event.ts),
    superseded_by: replacement,
  };
};

const applyDepAdded = (state: State, event: EventRecord): void => {
  const payload = asObject(event.payload);
  const blocker = asString(payload.blocker);
  if (!blocker) {
    throw new TsqError("INVALID_EVENT", "dep.added requires blocker", 1, {
      event_id: event.event_id,
    });
  }
  const deps = state.deps[event.task_id] ?? [];
  if (!deps.includes(blocker)) {
    state.deps[event.task_id] = [...deps, blocker];
  }
};

const applyDepRemoved = (state: State, event: EventRecord): void => {
  const payload = asObject(event.payload);
  const blocker = asString(payload.blocker);
  if (!blocker) {
    throw new TsqError("INVALID_EVENT", "dep.removed requires blocker", 1, {
      event_id: event.event_id,
    });
  }
  const deps = state.deps[event.task_id] ?? [];
  state.deps[event.task_id] = deps.filter((candidate) => candidate !== blocker);
};

const relationTarget = (payload: Record<string, unknown>): string | undefined => {
  return asString(payload.target) ?? asString(payload.dst) ?? asString(payload.to);
};

const applyLinkAdded = (state: State, event: EventRecord): void => {
  const payload = asObject(event.payload);
  const type = asRelationType(payload.type);
  const target = relationTarget(payload);
  if (!type || !target) {
    throw new TsqError("INVALID_EVENT", "link.added requires target and type", 1, {
      event_id: event.event_id,
    });
  }
  if (target === event.task_id) {
    throw new TsqError("RELATION_SELF_EDGE", "Relation self-edge is not allowed", 1, {
      task_id: event.task_id,
    });
  }
  upsertDirectedLink(state.links, event.task_id, target, type);
  if (type === "relates_to") {
    upsertDirectedLink(state.links, target, event.task_id, type);
  }
};

const applyLinkRemoved = (state: State, event: EventRecord): void => {
  const payload = asObject(event.payload);
  const type = asRelationType(payload.type);
  const target = relationTarget(payload);
  if (!type || !target) {
    throw new TsqError("INVALID_EVENT", "link.removed requires target and type", 1, {
      event_id: event.event_id,
    });
  }
  removeDirectedLink(state.links, event.task_id, target, type);
  if (type === "relates_to") {
    removeDirectedLink(state.links, target, event.task_id, type);
  }
};

export const applyEvent = (state: State, event: EventRecord): State => {
  const next = cloneState(state);

  switch (event.type) {
    case "task.created":
      applyTaskCreated(next, event);
      break;
    case "task.updated":
      applyTaskUpdated(next, event);
      break;
    case "task.claimed":
      applyTaskClaimed(next, event);
      break;
    case "task.superseded":
      applyTaskSuperseded(next, event);
      break;
    case "dep.added":
      applyDepAdded(next, event);
      break;
    case "dep.removed":
      applyDepRemoved(next, event);
      break;
    case "link.added":
      applyLinkAdded(next, event);
      break;
    case "link.removed":
      applyLinkRemoved(next, event);
      break;
    default:
      throw new TsqError("INVALID_EVENT_TYPE", "Unknown event type", 1, {
        event_id: event.event_id,
        type: event.type,
      });
  }

  next.applied_events += 1;
  return next;
};

export const applyEvents = (base: State, events: EventRecord[]): State => {
  let state = base;
  for (const event of events) {
    state = applyEvent(state, event);
  }
  return state;
};
