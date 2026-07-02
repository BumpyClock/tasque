export type TaskStatus =
	| "open"
	| "in_progress"
	| "blocked"
	| "deferred"
	| "closed"
	| "canceled";

export type TaskKind = "task" | "feature" | "epic";
export type SpecState = "attached" | "missing" | "invalid";
export type TabKey = "tasks" | "epics" | "board" | "deps";
export type BoardLane = "open" | "in_progress" | "done";

export const TASK_STATUS_ORDER: TaskStatus[] = [
	"in_progress",
	"open",
	"blocked",
	"deferred",
	"closed",
	"canceled",
];

export const TAB_ORDER: TabKey[] = ["tasks", "epics", "board", "deps"];

export interface TasqueTask {
	id: string;
	kind: TaskKind;
	title: string;
	status: TaskStatus;
	priority: number;
	assignee?: string;
	parent_id?: string;
	planning_state?: "needs_planning" | "planned";
	labels: string[];
	created_at: string;
	updated_at: string;
	spec_path?: string;
	spec_fingerprint?: string;
}

export interface TaskSummary {
	total: number;
	open: number;
	inProgress: number;
	blocked: number;
}

export interface EpicProgress {
	epic: TasqueTask;
	children: TasqueTask[];
	done: number;
	open: number;
	inProgress: number;
}

export const STATUS_ORDER = Object.fromEntries(
	TASK_STATUS_ORDER.map((status, index) => [status, index]),
) as Record<TaskStatus, number>;

export const STATUS_COLORS: Record<TaskStatus, string> = {
	open: "#58A6FF",
	in_progress: "#46C2A8",
	blocked: "#F07178",
	deferred: "#D4A65A",
	closed: "#7BC77E",
	canceled: "#7C8696",
};

export const KIND_COLORS: Record<TaskKind, string> = {
	task: "#D4A65A",
	feature: "#7BC77E",
	epic: "#E6A15C",
};

export const SPEC_COLORS: Record<SpecState, string> = {
	attached: "#7BC77E",
	missing: "#D4A65A",
	invalid: "#F07178",
};

export function sortTasks(tasks: TasqueTask[]): TasqueTask[] {
	return [...tasks].sort((left, right) => {
		const statusOrder = STATUS_ORDER[left.status] - STATUS_ORDER[right.status];
		if (statusOrder !== 0) {
			return statusOrder;
		}
		if (left.priority !== right.priority) {
			return left.priority - right.priority;
		}
		if (left.created_at !== right.created_at) {
			return left.created_at.localeCompare(right.created_at);
		}
		return left.id.localeCompare(right.id);
	});
}

export function computeSummary(tasks: TasqueTask[]): TaskSummary {
	const summary: TaskSummary = {
		total: tasks.length,
		open: 0,
		inProgress: 0,
		blocked: 0,
	};
	for (const task of tasks) {
		if (task.status === "open") {
			summary.open += 1;
		}
		if (task.status === "in_progress") {
			summary.inProgress += 1;
		}
		if (task.status === "blocked") {
			summary.blocked += 1;
		}
	}
	return summary;
}

export function specState(task: TasqueTask): SpecState {
	if (task.spec_path && task.spec_fingerprint) {
		return "attached";
	}
	if (!task.spec_path && !task.spec_fingerprint) {
		return "missing";
	}
	return "invalid";
}

export function shortSpecFingerprint(value: string | undefined): string {
	if (!value) {
		return "-";
	}
	return value.length <= 12 ? value : value.slice(0, 12);
}

export function boardLane(task: TasqueTask): BoardLane {
	if (task.status === "open" || task.status === "deferred") {
		return "open";
	}
	if (task.status === "in_progress" || task.status === "blocked") {
		return "in_progress";
	}
	return "done";
}

export function boardColumns(
	tasks: TasqueTask[],
): Record<BoardLane, TasqueTask[]> {
	const lanes: Record<BoardLane, TasqueTask[]> = {
		open: [],
		in_progress: [],
		done: [],
	};
	for (const task of tasks) {
		lanes[boardLane(task)].push(task);
	}
	return lanes;
}

export function buildEpicProgressList(tasks: TasqueTask[]): EpicProgress[] {
	const epics = tasks.filter((task) => task.kind === "epic");
	const childrenByParent = new Map<string, TasqueTask[]>();
	for (const task of tasks) {
		if (!task.parent_id) {
			continue;
		}
		const children = childrenByParent.get(task.parent_id) ?? [];
		children.push(task);
		childrenByParent.set(task.parent_id, children);
	}

	return epics.map((epic) => {
		const children = childrenByParent.get(epic.id) ?? [];

		let done = 0;
		let open = 0;
		let inProgress = 0;
		for (const child of children) {
			if (child.status === "closed" || child.status === "canceled") {
				done += 1;
			} else if (child.status === "in_progress" || child.status === "blocked") {
				inProgress += 1;
			} else {
				open += 1;
			}
		}

		return {
			epic,
			children,
			done,
			open,
			inProgress,
		};
	});
}

export function normalizeTab(value: string | undefined): TabKey {
	return TAB_ORDER.includes(value as TabKey) ? (value as TabKey) : "tasks";
}

export function statusLabel(status: TaskStatus): string {
	switch (status) {
		case "in_progress":
			return "in progress";
		default:
			return status;
	}
}

export function titleWithEllipsis(value: string, maxChars: number): string {
	if (maxChars <= 1) {
		return value.slice(0, 1);
	}
	if (value.length <= maxChars) {
		return value;
	}
	return `${value.slice(0, maxChars - 1)}…`;
}
