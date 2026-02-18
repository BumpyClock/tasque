import type { DepDirection } from "../domain/dep-tree";
import type { SkillOperationSummary, SkillTarget } from "../skills/types";
import type {
  EventRecord,
  Priority,
  RelationType,
  Task,
  TaskKind,
  TaskNote,
  TaskStatus,
} from "../types";

export interface InitResult {
  initialized: boolean;
  files: string[];
  skill_operation?: SkillOperationSummary;
}

export interface InitInput {
  installSkill?: boolean;
  uninstallSkill?: boolean;
  skillTargets?: SkillTarget[];
  skillName?: string;
  forceSkillOverwrite?: boolean;
  skillDirClaude?: string;
  skillDirCodex?: string;
  skillDirCopilot?: string;
  skillDirOpencode?: string;
}

export interface CreateInput {
  title: string;
  kind: TaskKind;
  priority: Priority;
  description?: string;
  externalRef?: string;
  parent?: string;
  exactId?: boolean;
}

export interface UpdateInput {
  id: string;
  title?: string;
  description?: string;
  clearDescription?: boolean;
  externalRef?: string;
  clearExternalRef?: boolean;
  status?: TaskStatus;
  priority?: Priority;
  exactId?: boolean;
}

export interface ClaimInput {
  id: string;
  assignee?: string;
  requireSpec?: boolean;
  exactId?: boolean;
}

export interface LinkInput {
  src: string;
  dst: string;
  type: RelationType;
  exactId?: boolean;
}

export interface DepInput {
  child: string;
  blocker: string;
  exactId?: boolean;
}

export interface SupersedeInput {
  source: string;
  withId: string;
  reason?: string;
  exactId?: boolean;
}

export interface DuplicateInput {
  source: string;
  canonical: string;
  reason?: string;
  exactId?: boolean;
}

export interface CloseInput {
  ids: string[];
  reason?: string;
  exactId?: boolean;
}

export interface ReopenInput {
  ids: string[];
  exactId?: boolean;
}

export interface HistoryInput {
  id: string;
  limit?: number;
  type?: string;
  actor?: string;
  since?: string;
  exactId?: boolean;
}

export interface HistoryResult {
  events: EventRecord[];
  count: number;
  truncated: boolean;
}

export interface DuplicateCandidatesResult {
  scanned: number;
  groups: Array<{
    key: string;
    tasks: Task[];
  }>;
}

export interface LabelInput {
  id: string;
  label: string;
  exactId?: boolean;
}

export interface DepTreeInput {
  id: string;
  direction?: DepDirection;
  depth?: number;
  exactId?: boolean;
}

export interface NoteAddInput {
  id: string;
  text: string;
  exactId?: boolean;
}

export interface NoteListInput {
  id: string;
  exactId?: boolean;
}

/** Input for attaching a markdown spec to a task. */
export interface SpecAttachInput {
  id: string;
  file?: string;
  source?: string;
  text?: string;
  stdin?: boolean;
  force?: boolean;
  exactId?: boolean;
}

export interface SpecCheckInput {
  id: string;
  exactId?: boolean;
}

export interface NoteAddResult {
  task_id: string;
  note: TaskNote;
  notes_count: number;
}

export interface NoteListResult {
  task_id: string;
  notes: TaskNote[];
}

export interface SpecAttachResult {
  task: Task;
  spec: {
    spec_path: string;
    spec_fingerprint: string;
    spec_attached_at: string;
    spec_attached_by: string;
    bytes: number;
  };
}

export type { SpecCheckDiagnostic, SpecCheckResult } from "./storage";
export type { SkillTarget } from "../skills/types";

export interface SearchInput {
  query: string;
}

/** Declarative filter applied to task listings. All fields are optional; unset fields match everything. */
export interface ListFilter {
  statuses?: TaskStatus[];
  assignee?: string;
  externalRef?: string;
  kind?: TaskKind;
  label?: string;
  labelAny?: string[];
  createdAfter?: string;
  updatedAfter?: string;
  closedAfter?: string;
  unassigned?: boolean;
  ids?: string[];
}

export interface StaleInput {
  days: number;
  status?: TaskStatus;
  assignee?: string;
}

export interface StaleResult {
  tasks: Task[];
  days: number;
  cutoff: string;
  statuses: TaskStatus[];
}

export interface DoctorResult {
  tasks: number;
  events: number;
  snapshot_loaded: boolean;
  warning?: string;
  issues: string[];
}

/** Constructor parameters shared by service method modules. */
export interface ServiceContext {
  repoRoot: string;
  actor: string;
  now: () => string;
}
