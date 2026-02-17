import { mkdir, open, readFile } from "node:fs/promises";
import { join } from "node:path";
import { ulid } from "ulid";
import { makeRootId, nextChildId } from "../domain/ids";
import { applyEvents } from "../domain/projector";
import { resolveTaskId } from "../domain/resolve";
import { assertNoDependencyCycle, isReady, listReady } from "../domain/validate";
import { TsqError } from "../errors";
import { writeDefaultConfig } from "../store/config";
import { appendEvents } from "../store/events";
import { withWriteLock } from "../store/lock";
import { getPaths } from "../store/paths";
import type {
  EventRecord,
  Priority,
  RelationType,
  State,
  Task,
  TaskKind,
  TaskStatus,
} from "../types";
import { loadProjectedState, persistProjection } from "./state";

export interface InitResult {
  initialized: boolean;
  files: string[];
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

  async init(): Promise<InitResult> {
    await writeDefaultConfig(this.repoRoot);
    await ensureEventsFile(this.repoRoot);
    await mkdir(join(this.repoRoot, ".tasque", "snapshots"), { recursive: true });
    await ensureTasqueGitignore(this.repoRoot);
    const files = [".tasque/config.json", ".tasque/events.jsonl", ".tasque/.gitignore"];
    return { initialized: true, files };
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
    let tasks = Object.values(state.tasks);
    if (filter.status) {
      tasks = tasks.filter((task) => task.status === filter.status);
    }
    if (filter.assignee) {
      tasks = tasks.filter((task) => task.assignee === filter.assignee);
    }
    if (filter.kind) {
      tasks = tasks.filter((task) => task.kind === filter.kind);
    }
    return sortTasks(tasks);
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

async function ensureTasqueGitignore(repoRoot: string): Promise<void> {
  const target = join(getPaths(repoRoot).tasqueDir, ".gitignore");
  const desired = ["state.json", ".lock", "snapshots/", "snapshots/*.tmp", "state.json.tmp"];
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
