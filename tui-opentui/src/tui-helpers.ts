import type { DependencyNode } from "./data";
import type { BoardLane, SpecState, TabKey, TasqueTask } from "./model";
import type { DependencyLine, FilterPreset, TableLayout, TreeLine } from "./tui-types";

export const THEME = {
  shellBg: "#0D1520",
  panelBg: "#111B29",
  raisedBg: "#162235",
  border: "#25364D",
  text: "#D7E3F4",
  muted: "#9DB0C8",
  dim: "#7388A3",
  focus: "#58A6FF",
  ok: "#7BC77E",
  row: "#101C2C",
  rowSelected: "#1A2C44",
  warning: "#F07178",
};

const TAB_ORDER: TabKey[] = ["tasks", "epics", "board", "deps"];

export function statusIcon(status: TasqueTask["status"]): string {
  switch (status) {
    case "open":
    case "deferred":
      return "○";
    case "in_progress":
    case "blocked":
      return "◐";
    case "closed":
    case "canceled":
      return "●";
  }
}

export function kindLabel(kind: TasqueTask["kind"]): string {
  switch (kind) {
    case "task":
      return "Task";
    case "feature":
      return "Feature";
    case "epic":
      return "Epic";
  }
}

export function planningLabel(value: TasqueTask["planning_state"] | undefined): string {
  return value === "planned" ? "Planned" : "Needs planning";
}

export function specLabel(value: SpecState): string {
  switch (value) {
    case "attached":
      return "Spec Attached";
    case "missing":
      return "No Spec";
    case "invalid":
      return "Spec Invalid";
  }
}

export function formatUpdatedAt(iso: string): string {
  const parsed = new Date(iso);
  if (Number.isNaN(parsed.getTime())) {
    return iso;
  }
  const shortMonths = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec",
  ];
  const year = `${parsed.getFullYear()}`.slice(2);
  const month = shortMonths[parsed.getMonth()] ?? "???";
  const day = `${parsed.getDate()}`.padStart(2, "0");
  return `${day} ${month} ${year}`;
}

export function treeDisplayId(taskId: string, depth: number): string {
  if (depth <= 0) {
    return taskId;
  }
  const dotIndex = taskId.indexOf(".");
  if (dotIndex >= 0 && dotIndex < taskId.length - 1) {
    return taskId.slice(dotIndex);
  }
  return taskId;
}

export function buildTreePrefix(line: TreeLine): string {
  if (line.depth <= 0) {
    return "";
  }
  const ancestors = buildAncestorPrefix(line.siblingTrail);
  const own = line.isLastSibling ? "└─" : "├─";
  return `${ancestors}${own} `;
}

function buildAncestorPrefix(siblingTrail: boolean[]): string {
  return siblingTrail.map((hasMoreSiblings) => (hasMoreSiblings ? "│ " : "  ")).join("");
}

export async function readSpecLines(
  specPath: string,
): Promise<{ lines: string[]; warning?: string }> {
  try {
    const text = await Bun.file(specPath).text();
    if (text.length === 0) {
      return { lines: ["(empty spec)"] };
    }
    return {
      lines: text.replaceAll("\r\n", "\n").split("\n"),
    };
  } catch (error) {
    return {
      lines: [],
      warning: `Failed to open spec: ${errorMessage(error)}`,
    };
  }
}

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return "unknown error";
}

export function buildFilterPresets(statusCsv: string): FilterPreset[] {
  const customStatuses = parseStatusCsv(statusCsv);
  const presets: FilterPreset[] = [];
  if (customStatuses.length > 0) {
    presets.push({
      id: "custom",
      label: `custom:${customStatuses.join(",")}`,
      statuses: customStatuses,
    });
  } else {
    presets.push({
      id: "active",
      label: "active",
      statuses: ["open", "in_progress"],
    });
  }
  presets.push({ id: "open", label: "open", statuses: ["open"] });
  presets.push({ id: "in_progress", label: "in_progress", statuses: ["in_progress"] });
  presets.push({ id: "closed", label: "closed", statuses: ["closed"] });
  presets.push({ id: "canceled", label: "canceled", statuses: ["canceled"] });
  presets.push({ id: "done", label: "done", statuses: ["closed", "canceled"] });
  presets.push({ id: "full", label: "full", statuses: undefined });
  return presets;
}

function parseStatusCsv(value: string): TasqueTask["status"][] {
  const allowed = new Set<TasqueTask["status"]>([
    "open",
    "in_progress",
    "blocked",
    "deferred",
    "closed",
    "canceled",
  ]);
  const parsed: TasqueTask["status"][] = [];
  for (const token of value.split(",")) {
    const status = token.trim() as TasqueTask["status"];
    if (!status || !allowed.has(status) || parsed.includes(status)) {
      continue;
    }
    parsed.push(status);
  }
  return parsed;
}

export function applyTaskFilter(
  tasks: TasqueTask[],
  statuses: TasqueTask["status"][] | undefined,
): TasqueTask[] {
  if (!statuses || statuses.length === 0) {
    return tasks;
  }
  return tasks.filter((task) => statuses.includes(task.status));
}

export function buildTreeLines(tasks: TasqueTask[]): TreeLine[] {
  const byParent = new Map<string, TasqueTask[]>();
  const byId = new Map(tasks.map((task) => [task.id, task]));

  for (const task of tasks) {
    const parent = task.parent_id;
    if (!parent || !byId.has(parent)) {
      continue;
    }
    const list = byParent.get(parent) ?? [];
    list.push(task);
    byParent.set(parent, list);
  }

  const roots = tasks.filter((task) => !task.parent_id || !byId.has(task.parent_id));
  const output: TreeLine[] = [];

  const walk = (
    task: TasqueTask,
    depth: number,
    siblingTrail: boolean[],
    isLastSibling: boolean,
  ) => {
    const children = byParent.get(task.id) ?? [];
    output.push({
      task,
      depth,
      isLastSibling,
      siblingTrail,
    });
    children.forEach((child, index) => {
      walk(
        child,
        depth + 1,
        [...siblingTrail, index < children.length - 1],
        index === children.length - 1,
      );
    });
  };

  for (const root of roots) {
    const rootIndex = roots.indexOf(root);
    walk(root, 0, [], rootIndex >= roots.length - 1);
  }

  return output;
}

export function flattenDependencyTree(root: DependencyNode | undefined): DependencyLine[] {
  if (!root) {
    return [];
  }
  const lines: DependencyLine[] = [];
  const walk = (node: DependencyNode, depth: number, path: string) => {
    const title = node.task?.title ? ` ${node.task.title}` : "";
    const status = node.task?.status ? `${statusIcon(node.task.status)} ` : "";
    const edge = node.depType && node.direction ? ` (${node.depType}, ${node.direction})` : "";
    const indent = " ".repeat(depth * 2);
    lines.push({
      key: `${path}:${node.id}:${depth}`,
      text: `${indent}${status}${node.id}${edge}${title}`,
    });
    node.children.forEach((child, index) => walk(child, depth + 1, `${path}.${index}`));
  };
  walk(root, 0, "root");
  return lines;
}

export function tableHeader(layout: TableLayout): string {
  const parts = [
    pad("ID", layout.idWidth),
    pad("Type", layout.typeWidth),
    pad("Title", layout.titleWidth),
  ];
  parts.push(pad("Pr", layout.priorityWidth));
  if (layout.showSpec) {
    parts.push("Spec");
  }
  return parts.join(" ");
}

export function buildTableLayout(width: number): TableLayout {
  const layout: TableLayout = {
    idWidth: 12,
    typeWidth: 8,
    titleWidth: 24,
    priorityWidth: 3,
    specWidth: 9,
    showSpec: true,
  };

  const minTitle = 14;
  const calcTitleWidth = () => {
    const widths = [2, layout.idWidth, layout.typeWidth, layout.priorityWidth];
    if (layout.showSpec) {
      widths.push(layout.specWidth);
    }
    const separators = widths.length;
    const used = widths.reduce((sum, value) => sum + value, 0) + separators;
    return width - used;
  };

  while (true) {
    const titleWidth = calcTitleWidth();
    if (titleWidth >= minTitle) {
      layout.titleWidth = titleWidth;
      break;
    }
    if (layout.showSpec) {
      layout.showSpec = false;
      continue;
    }
    layout.titleWidth = Math.max(10, titleWidth);
    break;
  }

  return layout;
}

export function pad(value: string, width: number): string {
  if (value.length >= width) {
    return value.slice(0, width);
  }
  return `${value}${" ".repeat(width - value.length)}`;
}

export function renderMeter(done: number, total: number, width: number): string {
  if (total <= 0) {
    return `[${"░".repeat(width)}]`;
  }
  const filled = Math.round((done / total) * width);
  const clamped = Math.min(width, Math.max(0, filled));
  return `[${"█".repeat(clamped)}${"░".repeat(width - clamped)}]`;
}

export function clampIndex(next: number, size: number): number {
  if (size <= 0) {
    return 0;
  }
  if (next < 0) {
    return 0;
  }
  if (next >= size) {
    return size - 1;
  }
  return next;
}

export function clampNumber(value: number, min: number, max: number): number {
  if (value < min) {
    return min;
  }
  if (value > max) {
    return max;
  }
  return value;
}

export function visibleRange(selectedIndex: number, total: number, budget: number): [number, number] {
  if (total <= budget) {
    return [0, total];
  }
  const half = Math.floor(budget / 2);
  let start = Math.max(0, selectedIndex - half);
  let end = start + budget;
  if (end > total) {
    end = total;
    start = end - budget;
  }
  return [start, end];
}

export function nextTab(tab: TabKey, direction: 1 | -1): TabKey {
  const index = TAB_ORDER.indexOf(tab);
  const safe = index >= 0 ? index : 0;
  const next = (safe + direction + TAB_ORDER.length) % TAB_ORDER.length;
  return TAB_ORDER[next]!;
}

export function previousLane(lane: BoardLane): BoardLane {
  if (lane === "done") {
    return "in_progress";
  }
  if (lane === "in_progress") {
    return "open";
  }
  return "done";
}

export function nextLane(lane: BoardLane): BoardLane {
  if (lane === "open") {
    return "in_progress";
  }
  if (lane === "in_progress") {
    return "done";
  }
  return "open";
}
