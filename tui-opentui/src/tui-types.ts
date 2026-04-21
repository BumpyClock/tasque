import type { DependencyNode } from "./data";
import type { BoardLane, TabKey, TasqueTask } from "./model";

export type SelectedByTab = Record<TabKey, number>;
export type SelectedByLane = Record<BoardLane, number>;

export interface TreeLine {
  task: TasqueTask;
  depth: number;
  isLastSibling: boolean;
  siblingTrail: boolean[];
}

export interface TableLayout {
  idWidth: number;
  typeWidth: number;
  titleWidth: number;
  priorityWidth: number;
  specWidth: number;
  showSpec: boolean;
}

export interface FilterPreset {
  id: string;
  label: string;
  statuses?: TasqueTask["status"][];
}

export interface SpecDialogState {
  taskId: string;
  taskTitle: string;
  specPath: string;
  lines: string[];
  warning?: string;
  offset: number;
  loading: boolean;
}

export type DependencyLine = { key: string; text: string };
export type DependencyRoot = DependencyNode | undefined;
