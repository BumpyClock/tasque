import { mkdir, open, readFile, readdir, unlink } from "node:fs/promises";
import { join } from "node:path";
import { ulid } from "ulid";
import { makeRootId, nextChildId } from "../domain/ids";
import { applyEvents } from "../domain/projector";
import { resolveTaskId } from "../domain/resolve";
import { assertNoDependencyCycle, isReady, listReady } from "../domain/validate";
import { TsqError } from "../errors";
import { applySkillOperation } from "../skills";
import type { SkillOperationSummary, SkillTarget } from "../skills/types";
import { writeDefaultConfig } from "../store/config";
import { appendEvents } from "../store/events";
import { forceRemoveLock, lockExists, withWriteLock } from "../store/lock";
import { getPaths } from "../store/paths";
import type {
  EventRecord,
  Priority,
  RelationType,
  RepairPlan,
  RepairResult,
  State,
  Task,
  TaskKind,
  TaskStatus,
  TaskTreeNode,
} from "../types";
import { loadProjectedState, persistProjection } from "./state";

export interface InitResult {
  initialized: boolean;
  files: string[];
  skill_operation?: SkillOperationSummary;
}

export interface InitInput {
  installSkill?: boolean;
  uninstallSkill?: boolean;
  skillTargets?: SkillTarget[];
  skillName?: string;
  forceSkillOverwrite?: boolean;
  skillDirClaude?: string;
  skillDirCodex?: string;
  skillDirCopilot?: string;
  skillDirOpencode?: string;
}

export interface CreateInput {
  title: string;
  kind: TaskKind;
  priority: Priority;
  parent?: string;
  exactId?: boolean;
}

export interface UpdateInput {
  id: string;
  title?: string;
  status?: TaskStatus;
  priority?: Priority;
  exactId?: boolean;
}

export interface ClaimInput {
  id: string;
  assignee?: string;
  exactId?: boolean;
}

export interface LinkInput {
  src: string;
  dst: string;
  type: RelationType;
  exactId?: boolean;
}

export interface DepInput {
  child: string;
  blocker: string;
  exactId?: boolean;
}

export interface SupersedeInput {
  source: string;
  withId: string;
  reason?: string;
  exactId?: boolean;
}

export interface ListFilter {
  status?: TaskStatus;
  statuses?: TaskStatus[];
  assignee?: string;
  kind?: TaskKind;
}

export interface DoctorResult {
  tasks: number;
  events: number;
  snapshot_loaded: boolean;
  warning?: string;
  issues: string[];
}

export class TasqueService {
  constructor(
    private readonly repoRoot: string,
    private readonly actor: string,
    private readonly now: () => string,
  ) {}

  async init(input: InitInput = {}): Promise<InitResult> {
    if (input.installSkill && input.uninstallSkill) {
      throw new TsqError(
        "VALIDATION_ERROR",
        "cannot combine --install-skill with --uninstall-skill",
        1,
      );
    }

    await writeDefaultConfig(this.repoRoot);
    await ensureEventsFile(this.repoRoot);
    await mkdir(join(this.repoRoot, ".tasque", "snapshots"), { recursive: true });
    await ensureTasqueGitignore(this.repoRoot);
    const files = [".tasque/config.json", ".tasque/events.jsonl", ".tasque/.gitignore"];
    const action = input.installSkill ? "install" : input.uninstallSkill ? "uninstall" : undefined;

    if (!action) {
      return { initialized: true, files };
    }

    const skill_operation = await applySkillOperation({
      action,
      skillName: input.skillName ?? "tasque",
      targets: input.skillTargets ?? ["claude", "codex", "copilot", "opencode"],
      force: Boolean(input.forceSkillOverwrite),
      targetDirOverrides: {
        ...(input.skillDirClaude ? { claude: input.skillDirClaude } : {}),
        ...(input.skillDirCodex ? { codex: input.skillDirCodex } : {}),
        ...(input.skillDirCopilot ? { copilot: input.skillDirCopilot } : {}),
        ...(input.skillDirOpencode ? { opencode: input.skillDirOpencode } : {}),
      },
    });

    return {
      initialized: true,
      files,
      skill_operation,
    };
  }

  async create(input: CreateInput): Promise<Task> {
    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const parentId = input.parent
        ? mustResolveExisting(state, input.parent, input.exactId)
        : undefined;
      if (parentId && !state.tasks[parentId]) {
        throw new TsqError("NOT_FOUND", `parent task not found: ${parentId}`, 1);
      }

      const id = parentId ? nextChildId(state, parentId) : uniqueRootId(state, input.title);
      const ts = this.now();
      const event = makeEvent(this.actor, ts, "task.created", id, {
        id,
        title: input.title,
        kind: input.kind,
        priority: input.priority,
        status: "open",
        parent_id: parentId,
      });

      const nextState = applyEvents(state, [event]);
      await appendEvents(this.repoRoot, [event]);
      await persistProjection(this.repoRoot, nextState, allEvents.length + 1);
      return mustTask(nextState, id);
    });
  }

  async show(
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
    const { state, allEvents } = await loadProjectedState(this.repoRoot);
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

  async list(filter: ListFilter): Promise<Task[]> {
    const { state } = await loadProjectedState(this.repoRoot);
    return sortTasks(applyListFilter(Object.values(state.tasks), filter));
  }

  async listTree(filter: ListFilter): Promise<TaskTreeNode[]> {
    const { state } = await loadProjectedState(this.repoRoot);
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

  async ready(): Promise<Task[]> {
    const { state } = await loadProjectedState(this.repoRoot);
    return sortTasks(listReady(state));
  }

  async doctor(): Promise<DoctorResult> {
    const { state, allEvents, warning, snapshot } = await loadProjectedState(this.repoRoot);
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

  async update(input: UpdateInput): Promise<Task> {
    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const id = mustResolveExisting(state, input.id, input.exactId);
      const existing = mustTask(state, id);
      const patch: Record<string, unknown> = {};

      if (input.title !== undefined) {
        patch.title = input.title;
      }
      if (input.priority !== undefined) {
        patch.priority = input.priority;
      }
      if (input.status !== undefined) {
        patch.status = input.status;
        if (input.status === "closed") {
          patch.closed_at = this.now();
        }
      }

      if (Object.keys(patch).length === 0) {
        throw new TsqError("VALIDATION_ERROR", "no update fields provided", 1);
      }

      if (existing.status === "canceled" && patch.status === "in_progress") {
        throw new TsqError("VALIDATION_ERROR", "cannot move canceled task to in_progress", 1);
      }

      const event = makeEvent(this.actor, this.now(), "task.updated", id, patch);
      const nextState = applyEvents(state, [event]);
      await appendEvents(this.repoRoot, [event]);
      await persistProjection(this.repoRoot, nextState, allEvents.length + 1);
      return mustTask(nextState, id);
    });
  }

  async claim(input: ClaimInput): Promise<Task> {
    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const id = mustResolveExisting(state, input.id, input.exactId);
      const existing = mustTask(state, id);
      if (existing.assignee) {
        throw new TsqError("CLAIM_CONFLICT", `task already assigned to ${existing.assignee}`, 1);
      }
      const assignee = input.assignee ?? this.actor;
      const event = makeEvent(this.actor, this.now(), "task.claimed", id, {
        assignee,
      });
      const nextState = applyEvents(state, [event]);
      await appendEvents(this.repoRoot, [event]);
      await persistProjection(this.repoRoot, nextState, allEvents.length + 1);
      return mustTask(nextState, id);
    });
  }

  async depAdd(input: DepInput): Promise<{ child: string; blocker: string }> {
    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const child = mustResolveExisting(state, input.child, input.exactId);
      const blocker = mustResolveExisting(state, input.blocker, input.exactId);
      if (child === blocker) {
        throw new TsqError("VALIDATION_ERROR", "task cannot depend on itself", 1);
      }
      assertNoDependencyCycle(state, child, blocker);
      const event = makeEvent(this.actor, this.now(), "dep.added", child, {
        blocker,
      });
      const nextState = applyEvents(state, [event]);
      await appendEvents(this.repoRoot, [event]);
      await persistProjection(this.repoRoot, nextState, allEvents.length + 1);
      return { child, blocker };
    });
  }

  async depRemove(input: DepInput): Promise<{ child: string; blocker: string }> {
    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const child = mustResolveExisting(state, input.child, input.exactId);
      const blocker = mustResolveExisting(state, input.blocker, input.exactId);
      const event = makeEvent(this.actor, this.now(), "dep.removed", child, {
        blocker,
      });
      const nextState = applyEvents(state, [event]);
      await appendEvents(this.repoRoot, [event]);
      await persistProjection(this.repoRoot, nextState, allEvents.length + 1);
      return { child, blocker };
    });
  }

  async linkAdd(input: LinkInput): Promise<{ src: string; dst: string; type: RelationType }> {
    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const src = mustResolveExisting(state, input.src, input.exactId);
      const dst = mustResolveExisting(state, input.dst, input.exactId);
      if (src === dst) {
        throw new TsqError("VALIDATION_ERROR", "self-edge not allowed", 1);
      }
      const event = makeEvent(this.actor, this.now(), "link.added", src, {
        type: input.type,
        target: dst,
      });
      const nextState = applyEvents(state, [event]);
      await appendEvents(this.repoRoot, [event]);
      await persistProjection(this.repoRoot, nextState, allEvents.length + 1);
      return { src, dst, type: input.type };
    });
  }

  async linkRemove(input: LinkInput): Promise<{ src: string; dst: string; type: RelationType }> {
    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const src = mustResolveExisting(state, input.src, input.exactId);
      const dst = mustResolveExisting(state, input.dst, input.exactId);
      if (src === dst) {
        throw new TsqError("VALIDATION_ERROR", "self-edge not allowed", 1);
      }
      const event = makeEvent(this.actor, this.now(), "link.removed", src, {
        type: input.type,
        target: dst,
      });
      const nextState = applyEvents(state, [event]);
      await appendEvents(this.repoRoot, [event]);
      await persistProjection(this.repoRoot, nextState, allEvents.length + 1);
      return { src, dst, type: input.type };
    });
  }

  async supersede(input: SupersedeInput): Promise<Task> {
    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const source = mustResolveExisting(state, input.source, input.exactId);
      const withId = mustResolveExisting(state, input.withId, input.exactId);
      if (source === withId) {
        throw new TsqError("VALIDATION_ERROR", "cannot supersede task with itself", 1);
      }
      const event = makeEvent(this.actor, this.now(), "task.superseded", source, {
        with: withId,
        reason: input.reason,
      });
      const nextState = applyEvents(state, [event]);
      await appendEvents(this.repoRoot, [event]);
      await persistProjection(this.repoRoot, nextState, allEvents.length + 1);
      return mustTask(nextState, source);
    });
  }

  async repair(opts: { fix: boolean; forceUnlock: boolean }): Promise<RepairResult> {
    if (opts.forceUnlock && !opts.fix) {
      throw new TsqError("VALIDATION_ERROR", "--force-unlock requires --fix", 1);
    }

    const { state } = await loadProjectedState(this.repoRoot);
    const paths = getPaths(this.repoRoot);
    const plan: RepairPlan = {
      orphaned_deps: [],
      orphaned_links: [],
      stale_temps: [],
      stale_lock: false,
      old_snapshots: [],
    };

    for (const [child, blockers] of Object.entries(state.deps)) {
      for (const blocker of blockers) {
        if (!state.tasks[child] || !state.tasks[blocker]) {
          plan.orphaned_deps.push({ child, blocker });
        }
      }
    }

    for (const [src, rels] of Object.entries(state.links)) {
      for (const [kind, targets] of Object.entries(rels)) {
        for (const target of targets ?? []) {
          if (!state.tasks[src] || !state.tasks[target]) {
            plan.orphaned_links.push({
              src,
              dst: target,
              type: kind as RelationType,
            });
          }
        }
      }
    }

    try {
      const entries = await readdir(paths.tasqueDir);
      for (const entry of entries) {
        if (entry.includes(".tmp")) {
          plan.stale_temps.push(entry);
        }
      }
    } catch {}

    plan.stale_lock = await lockExists(this.repoRoot);

    try {
      const snapEntries = await readdir(paths.snapshotsDir);
      const snapshots = snapEntries.filter((name) => name.endsWith(".json")).sort();
      if (snapshots.length > 5) {
        plan.old_snapshots = snapshots.slice(0, snapshots.length - 5);
      }
    } catch {}

    if (!opts.fix) {
      return { plan, applied: false, events_appended: 0, files_removed: 0 };
    }

    if (opts.forceUnlock && plan.stale_lock) {
      await forceRemoveLock(this.repoRoot);
    }

    return withWriteLock(this.repoRoot, async () => {
      const { state: lockedState, allEvents } = await loadProjectedState(this.repoRoot);
      const events: EventRecord[] = [];

      for (const dep of plan.orphaned_deps) {
        events.push(
          makeEvent(this.actor, this.now(), "dep.removed", dep.child, {
            blocker: dep.blocker,
          }),
        );
      }

      for (const link of plan.orphaned_links) {
        events.push(
          makeEvent(this.actor, this.now(), "link.removed", link.src, {
            type: link.type,
            target: link.dst,
          }),
        );
      }

      if (events.length > 0) {
        const nextState = applyEvents(lockedState, events);
        await appendEvents(this.repoRoot, events);
        await persistProjection(this.repoRoot, nextState, allEvents.length + events.length);
      }

      let filesRemoved = 0;
      for (const temp of plan.stale_temps) {
        try {
          await unlink(join(paths.tasqueDir, temp));
          filesRemoved++;
        } catch {}
      }

      for (const snap of plan.old_snapshots) {
        try {
          await unlink(join(paths.snapshotsDir, snap));
          filesRemoved++;
        } catch {}
      }

      return {
        plan,
        applied: true,
        events_appended: events.length,
        files_removed: filesRemoved,
      };
    });
  }
}

function makeEvent(
  actor: string,
  ts: string,
  type: EventRecord["type"],
  taskId: string,
  payload: Record<string, unknown>,
): EventRecord {
  return {
    event_id: ulid(),
    ts,
    actor,
    type,
    task_id: taskId,
    payload,
  };
}

function uniqueRootId(state: State, title: string): string {
  const maxAttempts = 10;
  for (let idx = 0; idx < maxAttempts; idx += 1) {
    const nonce = idx === 0 ? undefined : ulid();
    const id = makeRootId(title, nonce);
    if (!state.tasks[id]) {
      return id;
    }
  }
  throw new TsqError("ID_COLLISION", "unable to allocate unique task id", 2);
}

function mustTask(state: State, id: string): Task {
  const task = state.tasks[id];
  if (!task) {
    throw new TsqError("NOT_FOUND", `task not found: ${id}`, 1);
  }
  return task;
}

function mustResolveExisting(state: State, raw: string, exactId?: boolean): string {
  const id = resolveTaskId(state, raw, exactId);
  if (!state.tasks[id]) {
    throw new TsqError("NOT_FOUND", `task not found: ${raw}`, 1);
  }
  return id;
}

function sortTasks(tasks: Task[]): Task[] {
  return [...tasks].sort((a, b) => {
    if (a.priority !== b.priority) {
      return a.priority - b.priority;
    }
    if (a.created_at === b.created_at) {
      return a.id.localeCompare(b.id);
    }
    return a.created_at.localeCompare(b.created_at);
  });
}

function applyListFilter(tasks: Task[], filter: ListFilter): Task[] {
  const allowedStatuses = filter.status ? [filter.status] : filter.statuses;
  return tasks.filter((task) => {
    if (allowedStatuses && !allowedStatuses.includes(task.status)) {
      return false;
    }
    if (filter.assignee && task.assignee !== filter.assignee) {
      return false;
    }
    if (filter.kind && task.kind !== filter.kind) {
      return false;
    }
    return true;
  });
}

function buildDependentsByBlocker(deps: State["deps"]): Map<string, string[]> {
  const dependentsByBlocker = new Map<string, string[]>();
  for (const [child, blockers] of Object.entries(deps)) {
    for (const blocker of blockers) {
      const dependents = dependentsByBlocker.get(blocker);
      if (dependents) {
        dependents.push(child);
      } else {
        dependentsByBlocker.set(blocker, [child]);
      }
    }
  }
  return dependentsByBlocker;
}

function sortTaskIds(taskIds: string[]): string[] {
  return [...taskIds].sort((a, b) => a.localeCompare(b));
}

async function ensureTasqueGitignore(repoRoot: string): Promise<void> {
  const target = join(getPaths(repoRoot).tasqueDir, ".gitignore");
  const desired = ["tasks.jsonl", "tasks.jsonl.tmp*", ".lock", "snapshots/", "snapshots/*.tmp"];
  try {
    await Bun.write(target, `${desired.join("\n")}\n`);
  } catch (error) {
    throw new TsqError("IO_ERROR", "failed writing .tasque/.gitignore", 2, error);
  }
}

async function ensureEventsFile(repoRoot: string): Promise<void> {
  const paths = getPaths(repoRoot);
  await mkdir(paths.tasqueDir, { recursive: true });
  try {
    await readFile(paths.eventsFile, "utf8");
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code !== "ENOENT") {
      throw new TsqError("IO_ERROR", "failed reading events file", 2, error);
    }
    const handle = await open(paths.eventsFile, "a");
    await handle.close();
  }
}
