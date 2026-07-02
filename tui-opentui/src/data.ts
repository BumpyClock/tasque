import { type TabKey, type TasqueTask, normalizeTab } from "./model";

export interface TuiConfig {
	intervalSeconds: number;
	statusCsv: string;
	assignee?: string;
	initialTab: TabKey;
	tsqBin: string;
}

interface ListEnvelope {
	ok: boolean;
	data?: {
		tasks?: TasqueTask[];
	};
	error?: {
		code?: string;
		message?: string;
	};
}

export interface DataSnapshot {
	fetchedAt: string;
	tasks: TasqueTask[];
	warning?: string;
}

export interface DependencyNode {
	id: string;
	depType?: string;
	direction?: string;
	task?: TasqueTask;
	children: DependencyNode[];
}

const DEFAULT_STATUS = "open,in_progress,blocked,deferred,closed,canceled";

export function readConfigFromEnv(): TuiConfig {
	const intervalRaw = process.env.TSQ_TUI_INTERVAL ?? "2";
	const parsedInterval = Number.parseInt(intervalRaw, 10);
	const intervalSeconds = Number.isFinite(parsedInterval)
		? Math.min(60, Math.max(1, parsedInterval))
		: 2;

	const statusCsv = process.env.TSQ_TUI_STATUS?.trim() || DEFAULT_STATUS;
	const assigneeRaw = process.env.TSQ_TUI_ASSIGNEE?.trim();
	const initialTab = normalizeTab(process.env.TSQ_TUI_VIEW?.trim());
	const tsqBin = process.env.TSQ_TUI_BIN?.trim() || "tsq";

	return {
		intervalSeconds,
		statusCsv,
		assignee: assigneeRaw ? assigneeRaw : undefined,
		initialTab,
		tsqBin,
	};
}

export interface TsqSpawnResult {
	exitCode: number;
	stdout: string;
	stderr: string;
}

export async function runTsq(
	argv: string[],
	options: { timeoutMs?: number } = {},
): Promise<TsqSpawnResult> {
	const subprocess = Bun.spawn(argv, {
		stdin: "ignore",
		stdout: "pipe",
		stderr: "pipe",
		timeout: options.timeoutMs ?? 10_000,
		killSignal: "SIGKILL",
	});

	const [stdout, stderr, exitCode] = await Promise.all([
		new Response(subprocess.stdout).text().catch(() => ""),
		new Response(subprocess.stderr).text().catch(() => ""),
		subprocess.exited,
	]);

	return { exitCode: exitCode ?? -1, stdout, stderr };
}

export function spawnWarning(bin: string, error: unknown): string {
	const message =
		error instanceof Error && error.message ? error.message : "unknown error";
	return `Failed to run ${bin}: ${message}`;
}

export async function fetchTasks(config: TuiConfig): Promise<DataSnapshot> {
	const args = ["--json", "watch", "--once", "--status", config.statusCsv];
	if (config.assignee) {
		args.push("--assignee", config.assignee);
	}

	let result: TsqSpawnResult;
	try {
		result = await runTsq([config.tsqBin, ...args]);
	} catch (error) {
		return {
			fetchedAt: new Date().toISOString(),
			tasks: [],
			warning: spawnWarning(config.tsqBin, error),
		};
	}

	const fetchedAt = new Date().toISOString();

	if (result.exitCode !== 0) {
		return {
			fetchedAt,
			tasks: [],
			warning:
				result.stderr.trim() ||
				`Failed to run ${config.tsqBin} ${args.join(" ")}`,
		};
	}

	const parsed = parseTasksEnvelope(result.stdout);
	return {
		fetchedAt,
		tasks: parsed.tasks,
		warning: parsed.warning,
	};
}

export function parseTasksEnvelope(stdout: string): {
	tasks: TasqueTask[];
	warning?: string;
} {
	let payload: unknown;
	try {
		payload = JSON.parse(stdout);
	} catch {
		return {
			tasks: [],
			warning: "Unable to parse JSON output from tsq watch --once",
		};
	}

	if (!payload || typeof payload !== "object") {
		return {
			tasks: [],
			warning: "Unexpected payload from tsq watch --once",
		};
	}
	const envelope = payload as ListEnvelope;

	if (typeof envelope.ok !== "boolean") {
		return { tasks: [], warning: "Unexpected payload from tsq watch --once" };
	}

	if (!envelope.ok) {
		return {
			tasks: [],
			warning: envelope.error?.message ?? "tsq watch returned an error",
		};
	}

	const tasks = envelope.data?.tasks;
	if (tasks !== undefined && !Array.isArray(tasks)) {
		return { tasks: [], warning: "Task payload missing tasks array" };
	}

	return { tasks: tasks ?? [] };
}

interface DepEnvelope {
	ok: boolean;
	data?: {
		root?: unknown;
	};
	error?: {
		code?: string;
		message?: string;
	};
}

export async function fetchDependencyTree(
	tsqBin: string,
	taskId: string,
): Promise<{ root?: DependencyNode; warning?: string }> {
	let result: TsqSpawnResult;
	try {
		result = await runTsq([
			tsqBin,
			"--json",
			"deps",
			taskId,
			"--direction",
			"both",
			"--depth",
			"4",
		]);
	} catch (error) {
		return { warning: spawnWarning(tsqBin, error) };
	}

	if (result.exitCode !== 0) {
		return {
			warning: result.stderr.trim() || `Failed to run ${tsqBin} deps ${taskId}`,
		};
	}

	return parseDependencyEnvelope(result.stdout);
}

export function createInFlightGuard<T>(
	fetcher: () => Promise<T>,
): () => Promise<T | undefined> {
	let inFlight = false;
	return async () => {
		if (inFlight) {
			return undefined;
		}
		inFlight = true;
		try {
			return await fetcher();
		} finally {
			inFlight = false;
		}
	};
}

export function createLatestGuard(): <T>(
	fetcher: () => Promise<T>,
) => Promise<T | undefined> {
	let sequence = 0;
	return async <T>(fetcher: () => Promise<T>) => {
		sequence += 1;
		const ticket = sequence;
		try {
			const result = await fetcher();
			return ticket === sequence ? result : undefined;
		} catch (error) {
			if (ticket !== sequence) {
				return undefined;
			}
			throw error;
		}
	};
}

export function parseDependencyEnvelope(stdout: string): {
	root?: DependencyNode;
	warning?: string;
} {
	let payload: unknown;
	try {
		payload = JSON.parse(stdout);
	} catch {
		return { warning: "Unable to parse JSON output from tsq deps" };
	}

	if (!payload || typeof payload !== "object") {
		return { warning: "Unexpected payload from tsq deps" };
	}
	const envelope = payload as DepEnvelope;

	if (typeof envelope.ok !== "boolean") {
		return { warning: "Unexpected payload from tsq deps" };
	}

	if (!envelope.ok) {
		return { warning: envelope.error?.message ?? "tsq deps returned an error" };
	}

	const root = normalizeDependencyNode(envelope.data?.root);
	if (!root) {
		return { warning: "Dependency tree payload missing root node" };
	}

	return { root };
}

function normalizeDependencyNode(value: unknown): DependencyNode | undefined {
	if (!value || typeof value !== "object") {
		return undefined;
	}
	const item = value as Record<string, unknown>;
	const id = typeof item.id === "string" ? item.id : undefined;
	if (!id) {
		return undefined;
	}

	const task = normalizeTask(item.task);
	const rawChildren = Array.isArray(item.children) ? item.children : [];
	const children = rawChildren
		.map((child) => normalizeDependencyNode(child))
		.filter((node): node is DependencyNode => node !== undefined);

	return {
		id,
		depType: typeof item.dep_type === "string" ? item.dep_type : undefined,
		direction: typeof item.direction === "string" ? item.direction : undefined,
		task,
		children,
	};
}

function normalizeTask(value: unknown): TasqueTask | undefined {
	if (!value || typeof value !== "object") {
		return undefined;
	}
	const item = value as Record<string, unknown>;
	if (
		typeof item.id !== "string" ||
		typeof item.kind !== "string" ||
		typeof item.title !== "string" ||
		typeof item.status !== "string" ||
		typeof item.priority !== "number" ||
		!Array.isArray(item.labels) ||
		typeof item.created_at !== "string" ||
		typeof item.updated_at !== "string"
	) {
		return undefined;
	}

	return {
		id: item.id,
		kind: item.kind as TasqueTask["kind"],
		title: item.title,
		status: item.status as TasqueTask["status"],
		priority: item.priority,
		assignee: typeof item.assignee === "string" ? item.assignee : undefined,
		parent_id: typeof item.parent_id === "string" ? item.parent_id : undefined,
		planning_state:
			item.planning_state === "needs_planning" ||
			item.planning_state === "planned"
				? item.planning_state
				: undefined,
		labels: item.labels.filter(
			(label): label is string => typeof label === "string",
		),
		created_at: item.created_at,
		updated_at: item.updated_at,
		spec_path: typeof item.spec_path === "string" ? item.spec_path : undefined,
		spec_fingerprint:
			typeof item.spec_fingerprint === "string"
				? item.spec_fingerprint
				: undefined,
	};
}
