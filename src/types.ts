export const SCHEMA_VERSION = 1;

export type TaskKind = "task" | "feature" | "epic";
export type TaskStatus = "open" | "in_progress" | "blocked" | "closed" | "canceled" | "deferred";
export type PlanningState = "needs_planning" | "planned";
export type DependencyType = "blocks" | "starts_after";
export type RelationType = "relates_to" | "replies_to" | "duplicates" | "supersedes";
export type Priority = 0 | 1 | 2 | 3;

export interface TaskNote {
  event_id: string;
  ts: string;
  actor: string;
  text: string;
}

export interface Task {
  id: string;
  kind: TaskKind;
  title: string;
  description?: string;
  notes: TaskNote[];
  spec_path?: string;
  spec_fingerprint?: string;
  spec_attached_at?: string;
  spec_attached_by?: string;
  status: TaskStatus;
  priority: Priority;
  assignee?: string;
  external_ref?: string;
  discovered_from?: string;
  parent_id?: string;
  superseded_by?: string;
  duplicate_of?: string;
  planning_state?: PlanningState;
  replies_to?: string;
  labels: string[];
  created_at: string;
  updated_at: string;
  closed_at?: string;
}

export interface DependencyEdge {
  blocker: string;
  dep_type: DependencyType;
}

export interface DependencyRef {
  id: string;
  dep_type: DependencyType;
}

export interface TaskTreeNode {
  task: Task;
  blockers: string[];
  dependents: string[];
  blocker_edges?: DependencyRef[];
  dependent_edges?: DependencyRef[];
  children: TaskTreeNode[];
}

export type EventType =
  | "task.created"
  | "task.updated"
  | "task.status_set"
  | "task.claimed"
  | "task.noted"
  | "task.spec_attached"
  | "task.superseded"
  | "dep.added"
  | "dep.removed"
  | "link.added"
  | "link.removed";

/** Backward-compatible untyped event record. Payload is `Record<string, unknown>`. */
export interface EventRecord {
  /** Canonical event identifier field. */
  id?: string;
  /** Legacy alias retained for compatibility with existing logs/tools. */
  event_id?: string;
  ts: string;
  actor: string;
  type: EventType;
  task_id: string;
  payload: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Typed payload interfaces per event type
// ---------------------------------------------------------------------------

export interface TaskCreatedPayload {
  id: string;
  title: string;
  kind: TaskKind;
  priority: Priority;
  status: TaskStatus;
  parent_id?: string;
  assignee?: string;
  labels?: string[];
  external_ref?: string;
  discovered_from?: string;
  description?: string;
  planning_state?: PlanningState;
}

export interface TaskUpdatedPayload {
  title?: string;
  priority?: Priority;
  assignee?: string;
  labels?: string[];
  external_ref?: string;
  discovered_from?: string;
  description?: string;
  clear_description?: boolean;
  clear_external_ref?: boolean;
  clear_discovered_from?: boolean;
  kind?: TaskKind;
  duplicate_of?: string;
  planning_state?: PlanningState;
}

export interface TaskStatusSetPayload {
  status: TaskStatus;
  closed_at?: string;
  reason?: string;
}

export interface TaskClaimedPayload {
  assignee?: string;
}

export interface TaskNotedPayload {
  text: string;
}

export interface TaskSpecAttachedPayload {
  spec_path: string;
  spec_fingerprint: string;
  spec_attached_at: string;
  spec_attached_by: string;
}

export interface TaskSupersededPayload {
  /** The replacement task ID. Stored as `with` in the event payload. */
  with: string;
  reason?: string;
}

export interface DepAddedPayload {
  blocker: string;
  dep_type?: DependencyType;
}

export interface DepRemovedPayload {
  blocker: string;
  dep_type?: DependencyType;
}

export interface LinkAddedPayload {
  target: string;
  type: RelationType;
}

export interface LinkRemovedPayload {
  target: string;
  type: RelationType;
}

// ---------------------------------------------------------------------------
// Discriminated union: event type -> typed payload
// ---------------------------------------------------------------------------

interface TypedEventBase {
  id?: string;
  event_id?: string;
  ts: string;
  actor: string;
  task_id: string;
}

export type TypedEventRecord =
  | (TypedEventBase & { type: "task.created"; payload: TaskCreatedPayload })
  | (TypedEventBase & { type: "task.updated"; payload: TaskUpdatedPayload })
  | (TypedEventBase & { type: "task.status_set"; payload: TaskStatusSetPayload })
  | (TypedEventBase & { type: "task.claimed"; payload: TaskClaimedPayload })
  | (TypedEventBase & { type: "task.noted"; payload: TaskNotedPayload })
  | (TypedEventBase & { type: "task.spec_attached"; payload: TaskSpecAttachedPayload })
  | (TypedEventBase & { type: "task.superseded"; payload: TaskSupersededPayload })
  | (TypedEventBase & { type: "dep.added"; payload: DepAddedPayload })
  | (TypedEventBase & { type: "dep.removed"; payload: DepRemovedPayload })
  | (TypedEventBase & { type: "link.added"; payload: LinkAddedPayload })
  | (TypedEventBase & { type: "link.removed"; payload: LinkRemovedPayload });

/** Map from event type string to its typed payload interface. */
export type EventPayloadMap = {
  "task.created": TaskCreatedPayload;
  "task.updated": TaskUpdatedPayload;
  "task.status_set": TaskStatusSetPayload;
  "task.claimed": TaskClaimedPayload;
  "task.noted": TaskNotedPayload;
  "task.spec_attached": TaskSpecAttachedPayload;
  "task.superseded": TaskSupersededPayload;
  "dep.added": DepAddedPayload;
  "dep.removed": DepRemovedPayload;
  "link.added": LinkAddedPayload;
  "link.removed": LinkRemovedPayload;
};

export interface State {
  tasks: Record<string, Task>;
  deps: Record<string, DependencyEdge[]>;
  links: Record<string, Partial<Record<RelationType, string[]>>>;
  child_counters: Record<string, number>;
  created_order: string[];
  applied_events: number;
}

export interface Snapshot {
  taken_at: string;
  event_count: number;
  state: State;
}

export interface Config {
  schema_version: number;
  snapshot_every: number;
}

export interface EnvelopeOk<T> {
  schema_version: number;
  command: string;
  ok: true;
  data: T;
}

export interface EnvelopeErr {
  schema_version: number;
  command: string;
  ok: false;
  error: {
    code: string;
    message: string;
    details?: unknown;
  };
}

export type Envelope<T> = EnvelopeOk<T> | EnvelopeErr;

export interface RepairPlan {
  orphaned_deps: Array<{ child: string; blocker: string; dep_type: DependencyType }>;
  orphaned_links: Array<{ src: string; dst: string; type: RelationType }>;
  stale_temps: string[];
  stale_lock: boolean;
  old_snapshots: string[];
}

export interface RepairResult {
  plan: RepairPlan;
  applied: boolean;
  events_appended: number;
  files_removed: number;
}

export interface ResolveOptions {
  exactId?: boolean;
}

export interface RuntimeOptions {
  repoRoot: string;
  actor: string;
  now: () => string;
}
