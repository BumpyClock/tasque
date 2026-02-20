export type TaskKind = "task" | "feature" | "epic";

export type TaskStatus =
  | "open"
  | "in_progress"
  | "blocked"
  | "deferred"
  | "closed"
  | "canceled";

export type PlanningState = "needs_planning" | "planned";

export type SpecState = "attached" | "missing" | "invalid";

export type TabId = "tasks" | "epics" | "board";

export type BoardLane = "open" | "in_progress" | "done";

export interface TaskRecord {
  id: string;
  kind: TaskKind;
  title: string;
  status: TaskStatus;
  priority: number;
  assignee: string | null;
  parent_id: string | null;
  planning_state: PlanningState | null;
  labels: string[];
  created_at: string;
  updated_at: string;
  closed_at: string | null;
  spec_path: string | null;
  spec_fingerprint: string | null;
  spec_state: SpecState;
  spec_reason: string;
}

export interface DataSnapshot {
  repoRoot: string;
  source: string;
  loadedAt: string;
  tasks: TaskRecord[];
  warnings: string[];
}

export interface TsqEnvelopeOk {
  schema_version: number;
  command: string;
  ok: true;
  data: {
    tasks?: unknown[];
  };
}

export interface TsqEnvelopeErr {
  schema_version: number;
  command: string;
  ok: false;
  error: {
    code?: string;
    message?: string;
  };
}

export type TsqEnvelope = TsqEnvelopeOk | TsqEnvelopeErr;
