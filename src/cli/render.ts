import pc from "picocolors";
import type { RepairResult, Task, TaskTreeNode } from "../types";

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

  for (const node of nodes) {
    printTreeNode(node, "", false, false);
  }
}

function printTreeNode(node: TaskTreeNode, prefix: string, isLast: boolean, branch: boolean): void {
  const branchPrefix = branch ? `${prefix}${isLast ? "\\-- " : "|-- "}` : prefix;
  console.log(`${branchPrefix}${formatTreeLine(node)}`);

  const childPrefix = branch ? `${prefix}${isLast ? "    " : "|   "}` : prefix;
  for (let idx = 0; idx < node.children.length; idx += 1) {
    const child = node.children[idx];
    if (!child) {
      continue;
    }
    printTreeNode(child, childPrefix, idx === node.children.length - 1, true);
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
  const parts: string[] = [node.task.id, node.task.status, `p${node.task.priority}`];
  if (node.task.assignee) {
    parts.push(`@${node.task.assignee}`);
  }
  parts.push(node.task.title);
  if (node.blockers.length > 0) {
    parts.push(`blockers=${node.blockers.join(",")}`);
  }
  if (node.dependents.length > 0) {
    parts.push(`dependents=${node.dependents.join(",")}`);
  }
  return parts.join(" ");
}
