import { TsqError } from "../errors";
import type {
  DepAddedPayload,
  DepRemovedPayload,
  EventRecord,
  LinkAddedPayload,
  LinkRemovedPayload,
  Priority,
  RelationType,
  State,
  Task,
  TaskClaimedPayload,
  TaskCreatedPayload,
  TaskKind,
  TaskNote,
  TaskNotedPayload,
  TaskSpecAttachedPayload,
  TaskStatus,
  TaskStatusSetPayload,
  TaskSupersededPayload,
  TaskUpdatedPayload,
} from "../types";
import { assertNoDependencyCycle } from "./validate";

const RELATION_TYPES: RelationType[] = ["relates_to", "replies_to", "duplicates", "supersedes"];
const TASK_KINDS: TaskKind[] = ["task", "feature", "epic"];
const TASK_STATUSES: TaskStatus[] = ["open", "in_progress", "blocked", "closed", "canceled"];

const asObject = (value: unknown): Record<string, unknown> => {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return {};
  }
  return value as Record<string, unknown>;
};

/**
 * Extract and cast the event payload to a typed interface.
 * Runtime validation is still required by each handler; this cast provides
 * compile-time safety after validation has been performed.
 */
const extractPayload = <T>(value: unknown): Partial<T> & Record<string, unknown> => {
  return asObject(value) as Partial<T> & Record<string, unknown>;
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

const asBoolean = (value: unknown): boolean | undefined => {
  return typeof value === "boolean" ? value : undefined;
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

const eventIdentifier = (event: EventRecord): string => {
  const id = event.id ?? event.event_id;
  if (!id || id.length === 0) {
    throw new TsqError("INVALID_EVENT", "event requires id", 1, {
      type: event.type,
      task_id: event.task_id,
    });
  }
  return id;
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
  const payload = extractPayload<TaskCreatedPayload>(event.payload);
  const title = asString(payload.title);
  if (!title || title.length === 0) {
    throw new TsqError("INVALID_EVENT", "task.created requires a title", 1, {
      event_id: event.id ?? event.event_id,
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
    description: asString(payload.description),
    notes: [],
    kind,
    status,
    priority,
    assignee: asString(payload.assignee),
    external_ref: asString(payload.external_ref),
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
  const payload = extractPayload<TaskUpdatedPayload>(event.payload);
  const next: Task = {
    ...current,
    notes: [...(current.notes ?? [])],
    updated_at: event.ts,
  };

  const title = asString(payload.title);
  if (title !== undefined) {
    if (title.length === 0) {
      throw new TsqError("INVALID_EVENT", "task.updated title must not be empty", 1, {
        event_id: event.id ?? event.event_id,
      });
    }
    next.title = title;
  }
  const kind = asTaskKind(payload.kind);
  if (kind) {
    next.kind = kind;
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
  const duplicateOf = asString(payload.duplicate_of);
  if (duplicateOf !== undefined) {
    if (duplicateOf === event.task_id) {
      throw new TsqError("INVALID_EVENT", "task.updated duplicate_of cannot reference itself", 1, {
        event_id: event.id ?? event.event_id,
      });
    }
    next.duplicate_of = duplicateOf;
  }
  const description = asString(payload.description);
  const clearDescription = asBoolean(payload.clear_description);
  const externalRef = asString(payload.external_ref);
  const clearExternalRef = asBoolean(payload.clear_external_ref);
  if (description !== undefined && clearDescription) {
    throw new TsqError(
      "INVALID_EVENT",
      "task.updated cannot combine description with clear_description",
      1,
      {
        event_id: event.id ?? event.event_id,
      },
    );
  }
  if (description !== undefined) {
    next.description = description;
  }
  if (clearDescription === true) {
    next.description = undefined;
  }
  if (externalRef !== undefined && clearExternalRef) {
    throw new TsqError(
      "INVALID_EVENT",
      "task.updated cannot combine external_ref with clear_external_ref",
      1,
      {
        event_id: event.id ?? event.event_id,
      },
    );
  }
  if (externalRef !== undefined) {
    next.external_ref = externalRef;
  }
  if (clearExternalRef === true) {
    next.external_ref = undefined;
  }

  state.tasks[event.task_id] = next;
};

const TERMINAL_STATUSES: TaskStatus[] = ["closed", "canceled"];

const applyTaskStatusSet = (state: State, event: EventRecord): void => {
  const current = requireTask(state, event.task_id);
  const payload = extractPayload<TaskStatusSetPayload>(event.payload);
  const status = asTaskStatus(payload.status);
  if (!status) {
    throw new TsqError("INVALID_EVENT", "task.status_set requires a valid status", 1, {
      event_id: event.id ?? event.event_id,
    });
  }
  if (TERMINAL_STATUSES.includes(current.status) && status === "in_progress") {
    throw new TsqError(
      "INVALID_TRANSITION",
      `cannot transition from ${current.status} to in_progress`,
      1,
      { event_id: event.id ?? event.event_id, from: current.status, to: status },
    );
  }
  const closedAt = status === "closed" ? event.ts : undefined;
  state.tasks[event.task_id] = {
    ...current,
    status,
    updated_at: event.ts,
    closed_at: closedAt,
  };
};

const applyTaskClaimed = (state: State, event: EventRecord): void => {
  const current = requireTask(state, event.task_id);
  if (current.status === "closed" || current.status === "canceled") {
    throw new TsqError("INVALID_TRANSITION", `cannot claim task with status ${current.status}`, 1, {
      event_id: event.id ?? event.event_id,
      status: current.status,
    });
  }
  const payload = extractPayload<TaskClaimedPayload>(event.payload);
  const assignee = asString(payload.assignee) ?? event.actor;
  const nextStatus = current.status === "open" ? "in_progress" : current.status;
  state.tasks[event.task_id] = {
    ...current,
    assignee,
    status: nextStatus,
    updated_at: event.ts,
  };
};

const applyTaskNoted = (state: State, event: EventRecord): void => {
  const current = requireTask(state, event.task_id);
  const payload = extractPayload<TaskNotedPayload>(event.payload);
  const text = asString(payload.text);
  if (!text || text.length === 0) {
    throw new TsqError("INVALID_EVENT", "task.noted requires text", 1, {
      event_id: event.id ?? event.event_id,
    });
  }
  const note: TaskNote = {
    event_id: eventIdentifier(event),
    ts: event.ts,
    actor: event.actor,
    text,
  };
  state.tasks[event.task_id] = {
    ...current,
    notes: [...(current.notes ?? []), note],
    updated_at: event.ts,
  };
};

const applyTaskSpecAttached = (state: State, event: EventRecord): void => {
  const current = requireTask(state, event.task_id);
  const payload = extractPayload<TaskSpecAttachedPayload>(event.payload);
  const specPath = asString(payload.spec_path);
  const specFingerprint = asString(payload.spec_fingerprint);
  const specAttachedAt = asString(payload.spec_attached_at) ?? event.ts;
  const specAttachedBy = asString(payload.spec_attached_by) ?? event.actor;

  if (!specPath || !specFingerprint) {
    throw new TsqError(
      "INVALID_EVENT",
      "task.spec_attached requires spec_path and spec_fingerprint",
      1,
      { event_id: event.id ?? event.event_id },
    );
  }

  state.tasks[event.task_id] = {
    ...current,
    spec_path: specPath,
    spec_fingerprint: specFingerprint,
    spec_attached_at: specAttachedAt,
    spec_attached_by: specAttachedBy,
    updated_at: event.ts,
  };
};

const applyTaskSuperseded = (state: State, event: EventRecord): void => {
  const source = requireTask(state, event.task_id);
  const payload = extractPayload<TaskSupersededPayload>(event.payload);
  const replacement = asString(payload.with);
  if (!replacement) {
    throw new TsqError("INVALID_EVENT", "task.superseded requires replacement task", 1, {
      event_id: event.id ?? event.event_id,
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
  const payload = extractPayload<DepAddedPayload>(event.payload);
  const blocker = asString(payload.blocker);
  if (!blocker) {
    throw new TsqError("INVALID_EVENT", "dep.added requires blocker", 1, {
      event_id: event.id ?? event.event_id,
    });
  }
  requireTask(state, event.task_id);
  requireTask(state, blocker);
  assertNoDependencyCycle(state, event.task_id, blocker);
  const deps = state.deps[event.task_id] ?? [];
  if (!deps.includes(blocker)) {
    state.deps[event.task_id] = [...deps, blocker];
  }
};

const applyDepRemoved = (state: State, event: EventRecord): void => {
  const payload = extractPayload<DepRemovedPayload>(event.payload);
  const blocker = asString(payload.blocker);
  if (!blocker) {
    throw new TsqError("INVALID_EVENT", "dep.removed requires blocker", 1, {
      event_id: event.id ?? event.event_id,
    });
  }
  const deps = state.deps[event.task_id] ?? [];
  state.deps[event.task_id] = deps.filter((candidate) => candidate !== blocker);
};

const relationTarget = (payload: Record<string, unknown>): string | undefined => {
  return asString(payload.target);
};

const applyLinkAdded = (state: State, event: EventRecord): void => {
  const payload = extractPayload<LinkAddedPayload>(event.payload);
  const type = asRelationType(payload.type);
  const target = relationTarget(payload);
  if (!type || !target) {
    throw new TsqError("INVALID_EVENT", "link.added requires target and type", 1, {
      event_id: event.id ?? event.event_id,
    });
  }
  if (target === event.task_id) {
    throw new TsqError("RELATION_SELF_EDGE", "Relation self-edge is not allowed", 1, {
      task_id: event.task_id,
    });
  }
  requireTask(state, event.task_id);
  requireTask(state, target);
  upsertDirectedLink(state.links, event.task_id, target, type);
  if (type === "relates_to") {
    upsertDirectedLink(state.links, target, event.task_id, type);
  }
};

const applyLinkRemoved = (state: State, event: EventRecord): void => {
  const payload = extractPayload<LinkRemovedPayload>(event.payload);
  const type = asRelationType(payload.type);
  const target = relationTarget(payload);
  if (!type || !target) {
    throw new TsqError("INVALID_EVENT", "link.removed requires target and type", 1, {
      event_id: event.id ?? event.event_id,
    });
  }
  removeDirectedLink(state.links, event.task_id, target, type);
  if (type === "relates_to") {
    removeDirectedLink(state.links, target, event.task_id, type);
  }
};

const applyEventMut = (state: State, event: EventRecord): void => {
  switch (event.type) {
    case "task.created":
      applyTaskCreated(state, event);
      break;
    case "task.updated":
      applyTaskUpdated(state, event);
      break;
    case "task.status_set":
      applyTaskStatusSet(state, event);
      break;
    case "task.claimed":
      applyTaskClaimed(state, event);
      break;
    case "task.noted":
      applyTaskNoted(state, event);
      break;
    case "task.spec_attached":
      applyTaskSpecAttached(state, event);
      break;
    case "task.superseded":
      applyTaskSuperseded(state, event);
      break;
    case "dep.added":
      applyDepAdded(state, event);
      break;
    case "dep.removed":
      applyDepRemoved(state, event);
      break;
    case "link.added":
      applyLinkAdded(state, event);
      break;
    case "link.removed":
      applyLinkRemoved(state, event);
      break;
    default:
      throw new TsqError("INVALID_EVENT_TYPE", "Unknown event type", 1, {
        event_id: event.id ?? event.event_id,
        type: event.type,
      });
  }
  state.applied_events += 1;
};

export const applyEvent = (state: State, event: EventRecord): State => {
  const next = cloneState(state);
  applyEventMut(next, event);
  return next;
};

export const applyEvents = (base: State, events: EventRecord[]): State => {
  if (events.length === 0) {
    return base;
  }
  const state = cloneState(base);
  for (const event of events) {
    applyEventMut(state, event);
  }
  return state;
};
