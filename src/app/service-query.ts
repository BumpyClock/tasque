import { buildDependentsByBlocker } from "../domain/dep-tree";
import { evaluateQuery, parseQuery } from "../domain/query";
import { isReady, listReady } from "../domain/validate";
import { TsqError } from "../errors";
import type { EventRecord, Task, TaskTreeNode } from "../types";
import type {
  DoctorResult,
  HistoryInput,
  HistoryResult,
  ListFilter,
  SearchInput,
  ServiceContext,
  StaleInput,
  StaleResult,
} from "./service-types";
import {
  DEFAULT_STALE_STATUSES,
  applyListFilter,
  mustResolveExisting,
  mustTask,
  sortStaleTasks,
  sortTaskIds,
  sortTasks,
} from "./service-utils";
import { loadProjectedState } from "./storage";

export async function show(
  ctx: ServiceContext,
  idRaw: string,
  exactId?: boolean,
): Promise<{
  task: Task;
  blockers: string[];
  dependents: string[];
  ready: boolean;
  links: Record<string, string[]>;
  history: EventRecord[];
}> {
  const { state, allEvents } = await loadProjectedState(ctx.repoRoot);
  const id = mustResolveExisting(state, idRaw, exactId);
  const task = mustTask(state, id);
  const blockers = [...(state.deps[id] ?? [])];
  const dependents = Object.entries(state.deps)
    .filter(([, blockersForChild]) => blockersForChild.includes(id))
    .map(([child]) => child);
  const linksRaw = state.links[id] ?? {};
  const links: Record<string, string[]> = {};
  for (const [kind, values] of Object.entries(linksRaw)) {
    links[kind] = [...(values ?? [])];
  }

  const history = allEvents.filter((evt) => {
    if (evt.task_id === id) {
      return true;
    }
    const payloadTaskIds: string[] = [];
    for (const value of Object.values(evt.payload)) {
      if (typeof value === "string" && value.startsWith("tsq-")) {
        payloadTaskIds.push(value);
      }
    }
    return payloadTaskIds.includes(id);
  });

  return {
    task,
    blockers,
    dependents,
    ready: isReady(state, id),
    links,
    history,
  };
}

export async function list(ctx: ServiceContext, filter: ListFilter): Promise<Task[]> {
  const { state } = await loadProjectedState(ctx.repoRoot);
  return sortTasks(applyListFilter(Object.values(state.tasks), filter));
}

export async function stale(ctx: ServiceContext, input: StaleInput): Promise<StaleResult> {
  if (!Number.isInteger(input.days) || input.days < 0) {
    throw new TsqError("VALIDATION_ERROR", "days must be an integer >= 0", 1);
  }

  const { state } = await loadProjectedState(ctx.repoRoot);
  const nowValue = ctx.now();
  const nowMs = Date.parse(nowValue);
  if (!Number.isFinite(nowMs)) {
    throw new TsqError("INTERNAL_ERROR", `invalid current timestamp: ${nowValue}`, 2);
  }

  const cutoff = new Date(nowMs - input.days * 24 * 60 * 60 * 1000).toISOString();
  const statuses = input.status ? [input.status] : [...DEFAULT_STALE_STATUSES];
  const tasks = Object.values(state.tasks).filter((task) => {
    if (!statuses.includes(task.status)) {
      return false;
    }
    if (input.assignee && task.assignee !== input.assignee) {
      return false;
    }
    return task.updated_at <= cutoff;
  });

  return {
    tasks: sortStaleTasks(tasks),
    days: input.days,
    cutoff,
    statuses,
  };
}

export async function listTree(ctx: ServiceContext, filter: ListFilter): Promise<TaskTreeNode[]> {
  const { state } = await loadProjectedState(ctx.repoRoot);
  const filteredTasks = applyListFilter(Object.values(state.tasks), filter);
  const tasksById = new Map(filteredTasks.map((task) => [task.id, task]));
  const childrenByParent = new Map<string, Task[]>();
  const roots: Task[] = [];

  for (const task of filteredTasks) {
    const parentId = task.parent_id;
    if (parentId && tasksById.has(parentId)) {
      const siblings = childrenByParent.get(parentId);
      if (siblings) {
        siblings.push(task);
      } else {
        childrenByParent.set(parentId, [task]);
      }
      continue;
    }
    roots.push(task);
  }

  const dependentsByBlocker = buildDependentsByBlocker(state.deps);
  const buildNode = (task: Task): TaskTreeNode => {
    const blockers = sortTaskIds(state.deps[task.id] ?? []);
    const dependents = sortTaskIds(dependentsByBlocker.get(task.id) ?? []);
    const childTasks = sortTasks(childrenByParent.get(task.id) ?? []);
    return {
      task,
      blockers,
      dependents,
      children: childTasks.map((child) => buildNode(child)),
    };
  };

  return sortTasks(roots).map((task) => buildNode(task));
}

export async function ready(ctx: ServiceContext): Promise<Task[]> {
  const { state } = await loadProjectedState(ctx.repoRoot);
  return sortTasks(listReady(state));
}

export async function doctor(ctx: ServiceContext): Promise<DoctorResult> {
  const { state, allEvents, warning, snapshot } = await loadProjectedState(ctx.repoRoot);
  const issues: string[] = [];

  for (const [child, blockers] of Object.entries(state.deps)) {
    if (!state.tasks[child]) {
      issues.push(`dependency source missing: ${child}`);
    }
    for (const blocker of blockers) {
      if (!state.tasks[blocker]) {
        issues.push(`dependency blocker missing: ${child} -> ${blocker}`);
      }
    }
  }

  for (const [src, rels] of Object.entries(state.links)) {
    if (!state.tasks[src]) {
      issues.push(`relation source missing: ${src}`);
    }
    for (const [kind, targets] of Object.entries(rels)) {
      for (const target of targets ?? []) {
        if (!state.tasks[target]) {
          issues.push(`relation target missing: ${src} -[${kind}]-> ${target}`);
        }
      }
    }
  }

  return {
    tasks: Object.keys(state.tasks).length,
    events: allEvents.length,
    snapshot_loaded: Boolean(snapshot),
    warning,
    issues,
  };
}

export async function history(ctx: ServiceContext, input: HistoryInput): Promise<HistoryResult> {
  const { state, allEvents } = await loadProjectedState(ctx.repoRoot);
  const id = mustResolveExisting(state, input.id, input.exactId);

  let events = allEvents.filter((evt) => {
    if (evt.task_id === id) return true;
    for (const value of Object.values(evt.payload)) {
      if (typeof value === "string" && value === id) return true;
    }
    return false;
  });

  if (input.type) {
    events = events.filter((e) => e.type === input.type);
  }
  if (input.actor) {
    events = events.filter((e) => e.actor === input.actor);
  }
  if (input.since) {
    const since = input.since;
    events = events.filter((e) => e.ts >= since);
  }

  events.sort((a, b) => b.ts.localeCompare(a.ts));

  const limit = input.limit ?? 50;
  const truncated = events.length > limit;
  const limited = events.slice(0, limit);

  return { events: limited, count: limited.length, truncated };
}

export async function search(ctx: ServiceContext, input: SearchInput): Promise<Task[]> {
  const { state } = await loadProjectedState(ctx.repoRoot);
  const filter = parseQuery(input.query);
  return sortTasks(evaluateQuery(Object.values(state.tasks), filter, state));
}
