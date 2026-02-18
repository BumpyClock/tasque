import pc from "picocolors";
import type { TasqueService } from "../app/service";
import { TsqError } from "../errors";
import { errEnvelope, okEnvelope } from "../output";
import type { Task, TaskStatus, TaskTreeNode } from "../types";
import {
  formatMetaBadge,
  formatStatus,
  formatStatusText,
  renderTaskTree,
  truncateWithEllipsis,
} from "./render";
import { resolveDensity, resolveWidth } from "./terminal";

// ── Types ──────────────────────────────────────────────────────────────────

export interface WatchOptions {
  interval: number;
  statuses: TaskStatus[];
  assignee?: string;
  tree: boolean;
  once: boolean;
  json: boolean;
}

interface WatchSummary {
  total: number;
  open: number;
  in_progress: number;
  blocked: number;
}

interface WatchFrameData {
  frame_ts: string;
  interval_s: number;
  filters: { status: TaskStatus[]; assignee?: string };
  summary: WatchSummary;
  tasks: Task[];
}

type FrameResult =
  | { ok: true; data: WatchFrameData }
  | { ok: false; error: string; code: string; exitCode: number };

// ── Constants ──────────────────────────────────────────────────────────────

const ANSI_CLEAR = "\x1b[2J\x1b[H";
const STATUS_ORDER: Record<TaskStatus, number> = {
  in_progress: 0,
  open: 1,
  blocked: 2,
  closed: 3,
  canceled: 4,
};

// ── Public API ─────────────────────────────────────────────────────────────

export async function startWatch(service: TasqueService, options: WatchOptions): Promise<void> {
  validateOptions(options);

  if (options.once) {
    const frame = await loadFrame(service, options);
    outputFrame(frame, options);
    if (!frame.ok) {
      process.exitCode = frame.exitCode;
    }
    return;
  }

  let paused = false;
  let refreshing = false;
  let lastFrame: FrameResult | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  const isTTY = Boolean(process.stdout.isTTY);
  const isInteractive = isTTY && !options.json;

  const cleanup = (): void => {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    if (isInteractive && process.stdin.isRaw) {
      process.stdin.setRawMode(false);
      process.stdin.pause();
    }
  };

  const quit = (): void => {
    cleanup();
    process.exit(0);
  };

  const refresh = async (): Promise<void> => {
    if (refreshing) return;
    refreshing = true;
    try {
      const frame = await loadFrame(service, options);
      lastFrame = frame;
      outputFrame(frame, options, isTTY && !options.json);
    } catch (error) {
      // keep last good frame, show error
      const errFrame: FrameResult = {
        ok: false,
        error: `refresh failed: ${error instanceof Error ? error.message : String(error)}`,
        code: error instanceof TsqError ? error.code : "REFRESH_ERROR",
        exitCode: error instanceof TsqError ? error.exitCode : 2,
      };
      if (lastFrame?.ok) {
        outputFrame(lastFrame, options, isTTY && !options.json);
      }
      outputFrame(errFrame, options, false);
    } finally {
      refreshing = false;
    }
  };

  const scheduleNext = (): void => {
    timer = setTimeout(async () => {
      if (!paused) {
        await refresh();
      }
      scheduleNext();
    }, options.interval * 1000);
  };

  // Keyboard handler
  if (isInteractive) {
    process.stdin.setRawMode(true);
    process.stdin.resume();
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (key: string) => {
      const ch = key.toLowerCase();
      if (ch === "q" || ch === "\x03") {
        quit();
      } else if (ch === "r") {
        refresh();
      } else if (ch === "p") {
        paused = !paused;
        if (lastFrame) {
          outputFrame(lastFrame, options, true, paused);
        }
      }
    });
  }

  process.on("SIGINT", quit);
  process.on("SIGTERM", quit);

  // First frame
  await refresh();
  scheduleNext();
}

// ── Frame loading ──────────────────────────────────────────────────────────

async function loadFrame(service: TasqueService, options: WatchOptions): Promise<FrameResult> {
  try {
    const tasks = await service.list({
      statuses: options.statuses,
      assignee: options.assignee,
    });

    const sorted = sortWatchTasks(tasks);
    const summary = computeSummary(sorted);

    return {
      ok: true,
      data: {
        frame_ts: new Date().toISOString(),
        interval_s: options.interval,
        filters: {
          status: options.statuses,
          assignee: options.assignee,
        },
        summary,
        tasks: sorted,
      },
    };
  } catch (err) {
    if (err instanceof TsqError) {
      return { ok: false, error: err.message, code: err.code, exitCode: err.exitCode };
    }
    const message = err instanceof Error ? err.message : "unknown error";
    return { ok: false, error: message, code: "REFRESH_ERROR", exitCode: 2 };
  }
}

// ── Frame output ───────────────────────────────────────────────────────────

function outputFrame(
  frame: FrameResult,
  options: WatchOptions,
  clearScreen = false,
  paused = false,
): void {
  if (options.json) {
    outputJsonFrame(frame);
    return;
  }
  outputHumanFrame(frame, options, clearScreen, paused);
}

function outputJsonFrame(frame: FrameResult): void {
  if (frame.ok) {
    console.log(JSON.stringify(okEnvelope("tsq watch", frame.data)));
  } else {
    console.log(JSON.stringify(errEnvelope("tsq watch", frame.code, frame.error)));
  }
}

function outputHumanFrame(
  frame: FrameResult,
  options: WatchOptions,
  clearScreen: boolean,
  paused: boolean,
): void {
  const width = resolveWidth();

  if (clearScreen) {
    process.stdout.write(ANSI_CLEAR);
  }

  if (!frame.ok) {
    console.log(pc.yellow(`⚠ refresh failed: ${frame.error}`));
    return;
  }

  const { data } = frame;
  const lines: string[] = [];

  // Header
  lines.push(renderHeader(data, paused, width));

  // Summary
  lines.push(renderSummary(data.summary));

  // Separator
  lines.push(pc.dim("─".repeat(width)));

  // Tasks
  if (data.tasks.length === 0) {
    lines.push(pc.dim("no active tasks"));
  } else if (options.tree) {
    const treeNodes = buildWatchTree(data.tasks);
    const treeLines = renderTaskTree(treeNodes, { width });
    // Remove the summary line that renderTaskTree appends (we have our own)
    const withoutSummary = treeLines.filter((l) => !l.startsWith(pc.dim("total=")));
    lines.push(...withoutSummary);
  } else {
    lines.push(...renderFlatTasks(data.tasks, width));
  }

  // Separator
  lines.push(pc.dim("─".repeat(width)));

  // Footer (only if interactive TTY)
  if (process.stdout.isTTY) {
    const pauseLabel = paused ? "p resume" : "p pause";
    lines.push(pc.dim(`q quit  r refresh  ${pauseLabel}`));
  }

  for (const line of lines) {
    console.log(line);
  }
}

// ── Renderers ──────────────────────────────────────────────────────────────

function renderHeader(data: WatchFrameData, paused: boolean, width: number): string {
  const ts = data.frame_ts;
  const filterStr = `status:${data.filters.status.join(",")}${data.filters.assignee ? ` assignee:${data.filters.assignee}` : ""}`;
  const pauseTag = paused ? pc.yellow(" ⏸ paused") : "";
  const density = resolveDensity(width);

  if (density === "narrow") {
    const shortTs = `${ts.slice(11, 19)}Z`;
    return `${pc.bold("[tsq watch]")} ${pc.dim(`refreshed=${shortTs} interval=${data.interval_s}s`)}${pauseTag}`;
  }

  return `${pc.bold("[tsq watch]")}  ${pc.dim(`refreshed=${ts}  interval=${data.interval_s}s  filter=${filterStr}`)}${pauseTag}`;
}

function renderSummary(summary: WatchSummary): string {
  const parts = [
    `active=${summary.total}`,
    pc.blue(`in_progress=${summary.in_progress}`),
    pc.cyan(`open=${summary.open}`),
  ];
  if (summary.blocked > 0) {
    parts.push(pc.yellow(`blocked=${summary.blocked}`));
  } else {
    parts.push(`blocked=${summary.blocked}`);
  }
  return parts.join("  ");
}

function renderFlatTasks(tasks: Task[], width: number): string[] {
  const density = resolveDensity(width);
  const lines: string[] = [];

  for (const task of tasks) {
    const status = formatStatus(task.status);
    const statusText = formatStatusText(task.status);
    const meta = pc.dim(formatMetaBadge(task));
    const id = pc.bold(task.id);

    if (density === "narrow") {
      const titleWidth = Math.max(12, width - statusText.length - 1 - task.id.length - 1);
      lines.push(`${status} ${id} ${truncateWithEllipsis(task.title, titleWidth)}`);
      lines.push(`  ${meta}`);
    } else {
      lines.push(`${status}  ${id}  ${task.title}  ${meta}`);
    }
  }
  return lines;
}

// ── Helpers ────────────────────────────────────────────────────────────────

function validateOptions(options: WatchOptions): void {
  if (options.interval < 1 || options.interval > 60) {
    throw new TsqError("VALIDATION_ERROR", "interval must be between 1 and 60 seconds", 1);
  }
}

function sortWatchTasks(tasks: Task[]): Task[] {
  return [...tasks].sort((a, b) => {
    const sa = STATUS_ORDER[a.status] ?? 9;
    const sb = STATUS_ORDER[b.status] ?? 9;
    if (sa !== sb) return sa - sb;
    if (a.priority !== b.priority) return a.priority - b.priority;
    if (a.created_at !== b.created_at) return a.created_at.localeCompare(b.created_at);
    return a.id.localeCompare(b.id);
  });
}

function computeSummary(tasks: Task[]): WatchSummary {
  const summary: WatchSummary = { total: tasks.length, open: 0, in_progress: 0, blocked: 0 };
  for (const task of tasks) {
    if (task.status === "open") summary.open += 1;
    else if (task.status === "in_progress") summary.in_progress += 1;
    else if (task.status === "blocked") summary.blocked += 1;
  }
  return summary;
}

function buildWatchTree(tasks: Task[]): TaskTreeNode[] {
  const byId = new Map(tasks.map((t) => [t.id, t]));
  const childrenByParent = new Map<string, Task[]>();
  const roots: Task[] = [];

  for (const task of tasks) {
    if (task.parent_id && byId.has(task.parent_id)) {
      const siblings = childrenByParent.get(task.parent_id) ?? [];
      siblings.push(task);
      childrenByParent.set(task.parent_id, siblings);
    } else {
      roots.push(task);
    }
  }

  const buildNode = (task: Task): TaskTreeNode => {
    const children = (childrenByParent.get(task.id) ?? []).map(buildNode);
    return { task, children, blockers: [], dependents: [] };
  };

  return roots.map(buildNode);
}
