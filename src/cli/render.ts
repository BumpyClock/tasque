import pc from "picocolors";
import type { HistoryResult } from "../app/service";
import type { DepTreeNode } from "../domain/dep-tree";
import type { RepairResult, Task, TaskNote, TaskStatus, TaskTreeNode } from "../types";
import { type Density, resolveDensity, resolveWidth } from "./terminal";

type TreeDensity = Density;

interface TreeRenderOptions {
  width?: number;
}

export function printTaskList(tasks: Task[]): void {
  if (tasks.length === 0) {
    console.log(pc.dim("no tasks"));
    return;
  }

  const header = ["ID", "P", "KIND", "STATUS", "ASSIGNEE", "TITLE"];
  const rows = tasks.map((task) => [
    task.id,
    String(task.priority),
    task.kind,
    task.status,
    task.assignee ?? "-",
    task.title,
  ]);
  const table = [header, ...rows];
  const widths = header.map((_, idx) => Math.max(...table.map((row) => row[idx]?.length ?? 0)));

  for (let rowIdx = 0; rowIdx < table.length; rowIdx += 1) {
    const row = table[rowIdx];
    if (!row) {
      continue;
    }
    const line = row
      .map((cell, idx) => cell.padEnd(widths[idx] ?? cell.length))
      .join("  ")
      .trimEnd();
    if (rowIdx === 0) {
      console.log(pc.bold(line));
      continue;
    }
    console.log(line);
  }
}

export function printTask(task: Task): void {
  console.log(`${pc.bold(task.id)} ${task.title}`);
  console.log(`kind=${task.kind} status=${task.status} priority=${task.priority}`);
  if (task.assignee) {
    console.log(`assignee=${task.assignee}`);
  }
  if (task.external_ref) {
    console.log(`external_ref=${task.external_ref}`);
  }
  if (task.parent_id) {
    console.log(`parent=${task.parent_id}`);
  }
  if (task.superseded_by) {
    console.log(`superseded_by=${task.superseded_by}`);
  }
  if (task.duplicate_of) {
    console.log(`duplicate_of=${task.duplicate_of}`);
  }
  if (task.description) {
    console.log(`description=${task.description}`);
  }
  const noteCount = (task.notes ?? []).length;
  console.log(`notes=${noteCount}`);
  if (task.spec_path && task.spec_fingerprint) {
    const attachedBy = task.spec_attached_by ? ` by=${task.spec_attached_by}` : "";
    const attachedAt = task.spec_attached_at ? ` at=${task.spec_attached_at}` : "";
    console.log(`spec=${task.spec_path} sha256=${task.spec_fingerprint}${attachedBy}${attachedAt}`);
  }
}

export function printTaskTree(nodes: TaskTreeNode[]): void {
  for (const line of renderTaskTree(nodes)) {
    console.log(line);
  }
}

export function renderTaskTree(nodes: TaskTreeNode[], options: TreeRenderOptions = {}): string[] {
  if (nodes.length === 0) {
    return [pc.dim("no tasks")];
  }

  const width = resolveWidth(options.width);
  const density = resolveDensity(width);
  const lines: string[] = [];

  for (let idx = 0; idx < nodes.length; idx += 1) {
    const node = nodes[idx];
    if (!node) {
      continue;
    }
    renderTreeNode(lines, node, "", idx === nodes.length - 1, true, density, width);
  }

  const totals = summarizeTree(nodes);
  lines.push(
    pc.dim(
      `total=${totals.total} open=${totals.open} in_progress=${totals.in_progress} blocked=${totals.blocked} closed=${totals.closed} canceled=${totals.canceled}`,
    ),
  );
  return lines;
}

function renderTreeNode(
  lines: string[],
  node: TaskTreeNode,
  prefix: string,
  isLast: boolean,
  root: boolean,
  density: TreeDensity,
  width: number,
): void {
  const connector = root ? "" : isLast ? "└── " : "├── ";
  const linePrefix = `${prefix}${connector}`;
  const childPrefix = root ? prefix : `${prefix}${isLast ? "    " : "│   "}`;
  const metaPrefix = root ? (node.children.length > 0 ? "│   " : "    ") : childPrefix;
  const status = formatStatus(node.task.status);
  const statusText = formatStatusText(node.task.status);
  const flow = formatFlow(node);
  const primaryParts = [
    status,
    pc.bold(node.task.id),
    density === "narrow"
      ? truncateWithEllipsis(
          node.task.title,
          computeTitleWidth(width, linePrefix.length, statusText.length, node.task.id.length),
        )
      : node.task.title,
  ];
  if (density !== "narrow") {
    primaryParts.push(pc.dim(formatMetaBadge(node.task)));
  }
  if (density === "wide" && flow) {
    primaryParts.push(pc.dim(flow));
  }
  lines.push(`${linePrefix}${primaryParts.join(" ")}`);

  if (density === "medium" && flow) {
    lines.push(`${metaPrefix}${pc.dim(flow)}`);
  }
  if (density === "narrow") {
    lines.push(`${metaPrefix}${pc.dim(formatMetaBadge(node.task))}`);
    if (flow) {
      lines.push(`${metaPrefix}${pc.dim(flow)}`);
    }
  }

  for (let idx = 0; idx < node.children.length; idx += 1) {
    const child = node.children[idx];
    if (!child) {
      continue;
    }
    renderTreeNode(
      lines,
      child,
      childPrefix,
      idx === node.children.length - 1,
      false,
      density,
      width,
    );
  }
}

export function printRepairResult(result: RepairResult): void {
  if (result.applied) {
    console.log("mode=applied");
  } else {
    console.log("mode=dry-run (use --fix to apply)");
  }

  console.log(
    `orphaned_deps=${result.plan.orphaned_deps.length}${result.applied ? " (removed)" : ""}`,
  );
  for (const dep of result.plan.orphaned_deps) {
    console.log(`  ${dep.child} -> ${dep.blocker} (dep)`);
  }

  console.log(
    `orphaned_links=${result.plan.orphaned_links.length}${result.applied ? " (removed)" : ""}`,
  );
  for (const link of result.plan.orphaned_links) {
    console.log(`  ${link.src} -[${link.type}]-> ${link.dst}`);
  }

  console.log(`stale_temps=${result.plan.stale_temps.length}${result.applied ? " (deleted)" : ""}`);
  console.log(`stale_lock=${result.plan.stale_lock}`);
  console.log(
    `old_snapshots=${result.plan.old_snapshots.length}${result.applied && result.plan.old_snapshots.length > 0 ? " (pruned, kept last 5)" : ""}`,
  );

  if (result.applied) {
    console.log(`events_appended=${result.events_appended}`);
    console.log(`files_removed=${result.files_removed}`);
  }
}

export function formatMetaBadge(task: Task): string {
  return `[p${task.priority}${task.assignee ? ` @${task.assignee}` : ""}]`;
}

function formatFlow(node: TaskTreeNode): string | undefined {
  const flow: string[] = [];
  if (node.blockers.length > 0) {
    flow.push(`blocks-on: ${node.blockers.join(",")}`);
  }
  if (node.dependents.length > 0) {
    flow.push(`unblocks: ${node.dependents.join(",")}`);
  }
  if (flow.length === 0) {
    return undefined;
  }
  return `{${flow.join(" | ")}}`;
}

function computeTitleWidth(
  width: number,
  prefixLength: number,
  statusLength: number,
  taskIdLength: number,
): number {
  const fixedLength = prefixLength + statusLength + 1 + taskIdLength + 1;
  return Math.max(12, width - fixedLength);
}

export function truncateWithEllipsis(value: string, maxLength: number): string {
  if (value.length <= maxLength) {
    return value;
  }
  if (maxLength <= 3) {
    return value.slice(0, maxLength);
  }
  return `${value.slice(0, maxLength - 3)}...`;
}

export function formatStatusText(status: TaskStatus): string {
  switch (status) {
    case "open":
      return "○ open";
    case "in_progress":
      return "◐ in_progress";
    case "blocked":
      return "● blocked";
    case "closed":
      return "✓ closed";
    case "canceled":
      return "✕ canceled";
    default:
      return status;
  }
}

export function formatStatus(status: TaskStatus): string {
  switch (status) {
    case "open":
      return pc.cyan("○ open");
    case "in_progress":
      return pc.blue("◐ in_progress");
    case "blocked":
      return pc.yellow("● blocked");
    case "closed":
      return pc.green("✓ closed");
    case "canceled":
      return pc.red("✕ canceled");
    default:
      return status;
  }
}

function summarizeTree(nodes: TaskTreeNode[]): Record<TaskStatus | "total", number> {
  const summary: Record<TaskStatus | "total", number> = {
    total: 0,
    open: 0,
    in_progress: 0,
    blocked: 0,
    closed: 0,
    canceled: 0,
  };

  const visit = (node: TaskTreeNode): void => {
    summary.total += 1;
    summary[node.task.status] += 1;
    for (const child of node.children) {
      visit(child);
    }
  };

  for (const node of nodes) {
    visit(node);
  }
  return summary;
}

export function printHistory(data: HistoryResult): void {
  if (data.events.length === 0) {
    console.log(pc.dim("no events"));
    return;
  }
  for (const event of data.events) {
    console.log(`${event.ts} ${event.type} by=${event.actor} [${event.event_id}]`);
  }
  if (data.truncated) {
    console.log(pc.dim(`(showing ${data.count}, use --limit to see more)`));
  }
}

export function printLabelList(labels: Array<{ label: string; count: number }>): void {
  if (labels.length === 0) {
    console.log(pc.dim("no labels"));
    return;
  }
  for (const entry of labels) {
    console.log(`${entry.label} (${entry.count})`);
  }
}

export function printTaskNote(taskId: string, note: TaskNote): void {
  console.log(`${pc.bold(taskId)} note added`);
  console.log(`${note.ts} by=${note.actor} [${note.event_id}]`);
  console.log(note.text);
}

export function printTaskNotes(taskId: string, notes: TaskNote[]): void {
  if (notes.length === 0) {
    console.log(pc.dim(`${taskId}: no notes`));
    return;
  }
  console.log(`${pc.bold(taskId)} notes=${notes.length}`);
  for (const note of notes) {
    console.log(`${note.ts} by=${note.actor} [${note.event_id}]`);
    console.log(note.text);
  }
}

export function printDepTreeResult(root: DepTreeNode): void {
  printDepNode(root, "", true, true);
}

function printDepNode(node: DepTreeNode, prefix: string, isLast: boolean, isRoot: boolean): void {
  const connector = isRoot ? "" : isLast ? "└── " : "├── ";
  const dirTag = node.direction !== "both" ? pc.dim(` [${node.direction}]`) : "";
  console.log(
    `${prefix}${connector}${formatStatus(node.task.status)} ${pc.bold(node.task.id)} ${node.task.title}${dirTag}`,
  );

  const childPrefix = isRoot ? prefix : `${prefix}${isLast ? "    " : "│   "}`;
  for (let idx = 0; idx < node.children.length; idx += 1) {
    const child = node.children[idx];
    if (!child) continue;
    printDepNode(child, childPrefix, idx === node.children.length - 1, false);
  }
}
