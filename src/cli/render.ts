import pc from "picocolors";
import type { RepairResult, Task, TaskStatus, TaskTreeNode } from "../types";

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
  if (task.parent_id) {
    console.log(`parent=${task.parent_id}`);
  }
  if (task.superseded_by) {
    console.log(`superseded_by=${task.superseded_by}`);
  }
}

export function printTaskTree(nodes: TaskTreeNode[]): void {
  if (nodes.length === 0) {
    console.log(pc.dim("no tasks"));
    return;
  }

  for (let idx = 0; idx < nodes.length; idx += 1) {
    const node = nodes[idx];
    if (!node) {
      continue;
    }
    printTreeNode(node, "", idx === nodes.length - 1, true);
  }

  const totals = summarizeTree(nodes);
  console.log(
    pc.dim(
      `total=${totals.total} open=${totals.open} in_progress=${totals.in_progress} blocked=${totals.blocked} closed=${totals.closed} canceled=${totals.canceled}`,
    ),
  );
}

function printTreeNode(node: TaskTreeNode, prefix: string, isLast: boolean, root: boolean): void {
  const connector = root ? "" : isLast ? "└── " : "├── ";
  console.log(`${prefix}${connector}${formatTreeLine(node)}`);

  const childPrefix = root ? prefix : `${prefix}${isLast ? "    " : "│   "}`;
  for (let idx = 0; idx < node.children.length; idx += 1) {
    const child = node.children[idx];
    if (!child) {
      continue;
    }
    printTreeNode(child, childPrefix, idx === node.children.length - 1, false);
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

function formatTreeLine(node: TaskTreeNode): string {
  const parts: string[] = [
    formatStatus(node.task.status),
    pc.bold(node.task.id),
    node.task.title,
    pc.dim(`[p${node.task.priority}${node.task.assignee ? ` @${node.task.assignee}` : ""}]`),
  ];

  const flow: string[] = [];
  if (node.blockers.length > 0) {
    flow.push(`blocks-on: ${node.blockers.join(",")}`);
  }
  if (node.dependents.length > 0) {
    flow.push(`unblocks: ${node.dependents.join(",")}`);
  }
  if (flow.length > 0) {
    parts.push(pc.dim(`{${flow.join(" | ")}}`));
  }
  return parts.join(" ");
}

function formatStatus(status: TaskStatus): string {
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
