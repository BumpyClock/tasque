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

const DEFAULT_STATUS = "open,in_progress";

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

export function fetchTasks(config: TuiConfig): DataSnapshot {
  const args = ["--json", "list"];
  if (config.assignee) {
    args.push("--assignee", config.assignee);
  }

  const subprocess = Bun.spawnSync([config.tsqBin, ...args], {
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });

  const fetchedAt = new Date().toISOString();

  if (subprocess.exitCode !== 0) {
    const stderr = bytesToString(subprocess.stderr).trim();
    return {
      fetchedAt,
      tasks: [],
      warning: stderr || `Failed to run ${config.tsqBin} ${args.join(" ")}`,
    };
  }

  const stdout = bytesToString(subprocess.stdout);

  let payload: ListEnvelope;
  try {
    payload = JSON.parse(stdout) as ListEnvelope;
  } catch {
    return {
      fetchedAt,
      tasks: [],
      warning: "Unable to parse JSON output from tsq list",
    };
  }

  if (!payload.ok) {
    return {
      fetchedAt,
      tasks: [],
      warning: payload.error?.message ?? "tsq list returned an error",
    };
  }

  return {
    fetchedAt,
    tasks: payload.data?.tasks ?? [],
  };
}

function bytesToString(buffer: Uint8Array): string {
  return new TextDecoder().decode(buffer);
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

export function fetchDependencyTree(
  tsqBin: string,
  taskId: string,
): { root?: DependencyNode; warning?: string } {
  const subprocess = Bun.spawnSync(
    [
      tsqBin,
      "--json",
      "dep",
      "tree",
      taskId,
      "--direction",
      "both",
      "--depth",
      "4",
    ],
    {
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    },
  );

  if (subprocess.exitCode !== 0) {
    const stderr = bytesToString(subprocess.stderr).trim();
    return {
      warning: stderr || `Failed to run ${tsqBin} dep tree ${taskId}`,
    };
  }

  let payload: DepEnvelope;
  try {
    payload = JSON.parse(bytesToString(subprocess.stdout)) as DepEnvelope;
  } catch {
    return { warning: "Unable to parse JSON output from tsq dep tree" };
  }

  if (!payload.ok) {
    return { warning: payload.error?.message ?? "tsq dep tree returned an error" };
  }

  const root = normalizeDependencyNode(payload.data?.root);
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
      item.planning_state === "needs_planning" || item.planning_state === "planned"
        ? item.planning_state
        : undefined,
    labels: item.labels.filter((label): label is string => typeof label === "string"),
    created_at: item.created_at,
    updated_at: item.updated_at,
    spec_path: typeof item.spec_path === "string" ? item.spec_path : undefined,
    spec_fingerprint:
      typeof item.spec_fingerprint === "string" ? item.spec_fingerprint : undefined,
  };
}
