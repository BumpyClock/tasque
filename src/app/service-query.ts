import { buildDependentsByBlocker } from "../domain/dep-tree";
import { normalizeDependencyEdges } from "../domain/deps";
import { evaluateQuery, parseQuery } from "../domain/query";
import { type PlanningLane, isReady, listReady, listReadyByLane } from "../domain/validate";
import { TsqError } from "../errors";
import type { DependencyRef, DependencyType, EventRecord, Task, TaskTreeNode } from "../types";
import { scanOrphanedGraph } from "./repair";
import type {
  DoctorResult,
  HistoryInput,
  HistoryResult,
  ListFilter,
  OrphansResult,
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
  blocker_edges: DependencyRef[];
  dependent_edges: DependencyRef[];
  ready: boolean;
  links: Record<string, string[]>;
  history: EventRecord[];
}> {
  const { state, allEvents } = await loadProjectedState(ctx.repoRoot);
  const id = mustResolveExisting(state, idRaw, exactId);
  const task = mustTask(state, id);
  const blockerEdges = sortDependencyRefs(
    normalizeDependencyEdges(state.deps[id] as unknown).map((edge) => ({
      id: edge.blocker,
      dep_type: edge.dep_type,
    })),
  );
  const dependentsByBlocker = buildDependentsByBlocker(state.deps);
  const dependentEdges = sortDependencyRefs(
    (dependentsByBlocker.get(id) ?? []).map((edge) => ({
      id: edge.id,
      dep_type: edge.dep_type,
    })),
  );
  const blockers = [...new Set(blockerEdges.map((edge) => edge.id))];
  const dependents = [...new Set(dependentEdges.map((edge) => edge.id))];
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
    blocker_edges: blockerEdges,
    dependent_edges: dependentEdges,
    ready: isReady(state, id),
    links,
    history,
  };
}

export async function list(ctx: ServiceContext, filter: ListFilter): Promise<Task[]> {
  const { state } = await loadProjectedState(ctx.repoRoot);
  const base = applyListFilter(Object.values(state.tasks), filter);
  const depType = filter.depType;
  if (!depType) {
    return sortTasks(base);
  }
  const direction = filter.depDirection ?? "any";
  const dependentsByBlocker = buildDependentsByBlocker(state.deps);
  const filtered = base.filter((task) =>
    matchesDepTypeFilter(state, dependentsByBlocker, task.id, depType, direction),
  );
  return sortTasks(filtered);
}

export async function stale(ctx: ServiceContext, input: StaleInput): Promise<StaleResult> {
  if (!Number.isInteger(input.days) || input.days < 0) {
    throw new TsqError("VALIDATION_ERROR", "days must be an integer >= 0", 1);
  }
  if (input.limit !== undefined) {
    if (!Number.isInteger(input.limit) || input.limit < 1) {
      throw new TsqError("VALIDATION_ERROR", "limit must be an integer >= 1", 1);
    }
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

  const sorted = sortStaleTasks(tasks);
  const limited = input.limit !== undefined ? sorted.slice(0, input.limit) : sorted;
  return {
    tasks: limited,
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
    const blockerEdges = sortDependencyRefs(
      normalizeDependencyEdges(state.deps[task.id] as unknown).map((edge) => ({
        id: edge.blocker,
        dep_type: edge.dep_type,
      })),
    );
    const dependentEdges = sortDependencyRefs(
      (dependentsByBlocker.get(task.id) ?? []).map((edge) => ({
        id: edge.id,
        dep_type: edge.dep_type,
      })),
    );
    const blockers = sortTaskIds([...new Set(blockerEdges.map((edge) => edge.id))]);
    const dependents = sortTaskIds([...new Set(dependentEdges.map((edge) => edge.id))]);
    const childTasks = sortTasks(childrenByParent.get(task.id) ?? []);
    return {
      task,
      blockers,
      dependents,
      blocker_edges: blockerEdges,
      dependent_edges: dependentEdges,
      children: childTasks.map((child) => buildNode(child)),
    };
  };

  return sortTasks(roots).map((task) => buildNode(task));
}

export async function ready(ctx: ServiceContext, lane?: PlanningLane): Promise<Task[]> {
  const { state } = await loadProjectedState(ctx.repoRoot);
  if (lane) {
    return sortTasks(listReadyByLane(state, lane));
  }
  return sortTasks(listReady(state));
}

export async function doctor(ctx: ServiceContext): Promise<DoctorResult> {
  const { state, allEvents, warning, snapshot } = await loadProjectedState(ctx.repoRoot);
  const issues: string[] = [];

  for (const [child, blockers] of Object.entries(state.deps)) {
    if (!state.tasks[child]) {
      issues.push(`dependency source missing: ${child}`);
    }
    for (const edge of normalizeDependencyEdges(blockers as unknown)) {
      if (!state.tasks[edge.blocker]) {
        issues.push(`dependency blocker missing: ${child} -> ${edge.blocker} (${edge.dep_type})`);
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

function sortDependencyRefs(refs: DependencyRef[]): DependencyRef[] {
  return [...refs].sort((a, b) => {
    if (a.id === b.id) {
      return a.dep_type.localeCompare(b.dep_type);
    }
    return a.id.localeCompare(b.id);
  });
}

function matchesDepTypeFilter(
  state: { deps: Record<string, unknown> },
  dependentsByBlocker: Map<string, Array<{ id: string; dep_type: DependencyType }>>,
  taskId: string,
  depType: DependencyType,
  direction: "in" | "out" | "any",
): boolean {
  const hasOut = normalizeDependencyEdges(state.deps[taskId]).some(
    (edge) => edge.dep_type === depType,
  );
  const hasIn = (dependentsByBlocker.get(taskId) ?? []).some((edge) => edge.dep_type === depType);
  if (direction === "out") {
    return hasOut;
  }
  if (direction === "in") {
    return hasIn;
  }
  return hasOut || hasIn;
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

export async function orphans(ctx: ServiceContext): Promise<OrphansResult> {
  const { state } = await loadProjectedState(ctx.repoRoot);
  const scan = scanOrphanedGraph(state);
  return {
    orphaned_deps: scan.orphaned_deps,
    orphaned_links: scan.orphaned_links,
    total: scan.orphaned_deps.length + scan.orphaned_links.length,
  };
}
