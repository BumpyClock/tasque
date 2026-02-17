import pc from "picocolors";
import type { Task } from "../types";

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
