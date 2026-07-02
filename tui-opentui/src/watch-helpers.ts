import { type TasqueTask, sortTasks } from "./model";
import { buildTreePrefix, buildTreeLines } from "./tui-helpers";

export interface WatchRow {
	task: TasqueTask;
	prefix: string;
}

// Build the ordered rows for the watch list. Flat mode is a plain sorted list;
// tree mode reuses the shared tree builder so parent/child indentation matches
// `tsq watch --tree` and the Tasks tab of `tsq tui`.
export function buildWatchRows(
	tasks: TasqueTask[],
	tree: boolean,
): WatchRow[] {
	const sorted = sortTasks(tasks);
	if (!tree) {
		return sorted.map((task) => ({ task, prefix: "" }));
	}
	return buildTreeLines(sorted).map((line) => ({
		task: line.task,
		prefix: buildTreePrefix(line),
	}));
}

// Mirror the filter string rendered by the Rust watch header
// (`status:open,in_progress assignee:foo`).
export function filterLabel(statusCsv: string, assignee?: string): string {
	const base = `status:${statusCsv}`;
	return assignee ? `${base} assignee:${assignee}` : base;
}

// Compact priority/assignee badge shown at the end of each row (`[p2] @foo`).
export function metaBadge(task: TasqueTask): string {
	const parts = [`p${task.priority}`];
	if (task.assignee) {
		parts.push(`@${task.assignee}`);
	}
	return parts.join(" ");
}

export function readWatchTreeFlag(): boolean {
	return process.env.TSQ_WATCH_TREE?.trim() === "1";
}
