export const SCHEMA_VERSION = 1;

export type TaskKind = "task" | "feature" | "epic";
export type TaskStatus = "open" | "in_progress" | "blocked" | "closed" | "canceled";
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
  status: TaskStatus;
  priority: Priority;
  assignee?: string;
  parent_id?: string;
  superseded_by?: string;
  duplicate_of?: string;
  replies_to?: string;
  labels: string[];
  created_at: string;
  updated_at: string;
  closed_at?: string;
}

export interface TaskTreeNode {
  task: Task;
  blockers: string[];
  dependents: string[];
  children: TaskTreeNode[];
}

export interface EventRecord {
  event_id: string;
  ts: string;
  actor: string;
  type:
    | "task.created"
    | "task.updated"
    | "task.claimed"
    | "task.noted"
    | "task.superseded"
    | "dep.added"
    | "dep.removed"
    | "link.added"
    | "link.removed";
  task_id: string;
  payload: Record<string, unknown>;
}

export interface State {
  tasks: Record<string, Task>;
  deps: Record<string, string[]>;
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
  orphaned_deps: Array<{ child: string; blocker: string }>;
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
