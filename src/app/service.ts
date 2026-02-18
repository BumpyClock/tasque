import { createHash } from "node:crypto";
import { mkdir, open, readFile, rename } from "node:fs/promises";
import { dirname, join } from "node:path";
import { ulid } from "ulid";
import { buildDepTree } from "../domain/dep-tree";
import type { DepDirection, DepTreeNode } from "../domain/dep-tree";
import { makeRootId, nextChildId } from "../domain/ids";
import { addLabel, removeLabel } from "../domain/labels";
import { applyEvents } from "../domain/projector";
import { evaluateQuery, parseQuery } from "../domain/query";
import { resolveTaskId } from "../domain/resolve";
import { assertNoDependencyCycle, isReady, listReady } from "../domain/validate";
import { TsqError } from "../errors";
import { applySkillOperation } from "../skills";
import type { SkillOperationSummary, SkillTarget } from "../skills/types";
import { writeDefaultConfig } from "../store/config";
import { appendEvents } from "../store/events";
import { withWriteLock } from "../store/lock";
import { getPaths, taskSpecFile, taskSpecRelativePath } from "../store/paths";
import type {
  EventRecord,
  Priority,
  RelationType,
  RepairResult,
  State,
  Task,
  TaskKind,
  TaskNote,
  TaskStatus,
  TaskTreeNode,
} from "../types";
import { executeRepair } from "./repair";
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
  description?: string;
  parent?: string;
  exactId?: boolean;
}

export interface UpdateInput {
  id: string;
  title?: string;
  description?: string;
  clearDescription?: boolean;
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

export interface CloseInput {
  ids: string[];
  reason?: string;
  exactId?: boolean;
}

export interface ReopenInput {
  ids: string[];
  exactId?: boolean;
}

export interface HistoryInput {
  id: string;
  limit?: number;
  type?: string;
  actor?: string;
  since?: string;
  exactId?: boolean;
}

export interface HistoryResult {
  events: EventRecord[];
  count: number;
  truncated: boolean;
}

export interface LabelInput {
  id: string;
  label: string;
  exactId?: boolean;
}

export interface DepTreeInput {
  id: string;
  direction?: DepDirection;
  depth?: number;
  exactId?: boolean;
}

export interface NoteAddInput {
  id: string;
  text: string;
  exactId?: boolean;
}

export interface NoteListInput {
  id: string;
  exactId?: boolean;
}

export interface SpecAttachInput {
  id: string;
  file?: string;
  source?: string;
  text?: string;
  stdin?: boolean;
  exactId?: boolean;
}

export interface NoteAddResult {
  task_id: string;
  note: TaskNote;
  notes_count: number;
}

export interface NoteListResult {
  task_id: string;
  notes: TaskNote[];
}

export interface SpecAttachResult {
  task: Task;
  spec: {
    spec_path: string;
    spec_fingerprint: string;
    spec_attached_at: string;
    spec_attached_by: string;
    bytes: number;
  };
}

export interface SearchInput {
  query: string;
}

export interface ListFilter {
  status?: TaskStatus;
  statuses?: TaskStatus[];
  assignee?: string;
  kind?: TaskKind;
  label?: string;
}

export interface StaleInput {
  days: number;
  status?: TaskStatus;
  assignee?: string;
}

export interface StaleResult {
  tasks: Task[];
  days: number;
  cutoff: string;
  statuses: TaskStatus[];
}

export interface DoctorResult {
  tasks: number;
  events: number;
  snapshot_loaded: boolean;
  warning?: string;
  issues: string[];
}

const DEFAULT_STALE_STATUSES: TaskStatus[] = ["open", "in_progress", "blocked"];

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
    await mkdir(join(this.repoRoot, ".tasque", "snapshots"), {
      recursive: true,
    });
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
        description: input.description,
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

  async stale(input: StaleInput): Promise<StaleResult> {
    if (!Number.isInteger(input.days) || input.days < 0) {
      throw new TsqError("VALIDATION_ERROR", "days must be an integer >= 0", 1);
    }

    const { state } = await loadProjectedState(this.repoRoot);
    const nowValue = this.now();
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
    if (input.description !== undefined && input.clearDescription) {
      throw new TsqError(
        "VALIDATION_ERROR",
        "cannot combine --description with --clear-description",
        1,
      );
    }

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
      if (input.description !== undefined) {
        patch.description = input.description;
      }
      if (input.clearDescription) {
        patch.clear_description = true;
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

  async noteAdd(input: NoteAddInput): Promise<NoteAddResult> {
    const text = input.text.trim();
    if (text.length === 0) {
      throw new TsqError("VALIDATION_ERROR", "note text must not be empty", 1);
    }

    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const id = mustResolveExisting(state, input.id, input.exactId);
      const event = makeEvent(this.actor, this.now(), "task.noted", id, {
        text,
      });
      const nextState = applyEvents(state, [event]);
      await appendEvents(this.repoRoot, [event]);
      await persistProjection(this.repoRoot, nextState, allEvents.length + 1);
      const task = mustTask(nextState, id);
      const note = (task.notes ?? []).at(-1);
      if (!note) {
        throw new TsqError("INTERNAL_ERROR", "task note was not persisted", 2);
      }
      return {
        task_id: id,
        note,
        notes_count: task.notes.length,
      };
    });
  }

  async noteList(input: NoteListInput): Promise<NoteListResult> {
    const { state } = await loadProjectedState(this.repoRoot);
    const id = mustResolveExisting(state, input.id, input.exactId);
    const task = mustTask(state, id);
    return {
      task_id: id,
      notes: [...(task.notes ?? [])],
    };
  }

  async specAttach(input: SpecAttachInput): Promise<SpecAttachResult> {
    const source = resolveSpecAttachSource(input);
    const sourceContent = await readSpecAttachContent(source);
    if (sourceContent.trim().length === 0) {
      throw new TsqError("VALIDATION_ERROR", "spec markdown content must not be empty", 1);
    }

    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const id = mustResolveExisting(state, input.id, input.exactId);
      const specFile = await writeTaskSpecAtomic(this.repoRoot, id, sourceContent);
      const fingerprint = sha256(specFile.content);
      const attachedAt = this.now();
      const attachedBy = this.actor;

      const event = makeEvent(this.actor, attachedAt, "task.spec_attached", id, {
        spec_path: specFile.specPath,
        spec_fingerprint: fingerprint,
        spec_attached_at: attachedAt,
        spec_attached_by: attachedBy,
      });
      const nextState = applyEvents(state, [event]);
      await appendEvents(this.repoRoot, [event]);
      await persistProjection(this.repoRoot, nextState, allEvents.length + 1);

      return {
        task: mustTask(nextState, id),
        spec: {
          spec_path: specFile.specPath,
          spec_fingerprint: fingerprint,
          spec_attached_at: attachedAt,
          spec_attached_by: attachedBy,
          bytes: Buffer.byteLength(specFile.content, "utf8"),
        },
      };
    });
  }

  async claim(input: ClaimInput): Promise<Task> {
    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const id = mustResolveExisting(state, input.id, input.exactId);
      const existing = mustTask(state, id);
      const CLAIMABLE_STATUSES = ["open", "in_progress"];
      if (!CLAIMABLE_STATUSES.includes(existing.status)) {
        throw new TsqError(
          "INVALID_STATUS",
          `cannot claim task with status '${existing.status}'`,
          1,
        );
      }
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

  async repair(opts: {
    fix: boolean;
    forceUnlock: boolean;
  }): Promise<RepairResult> {
    return executeRepair(this.repoRoot, this.actor, this.now, opts);
  }

  async close(input: CloseInput): Promise<Task[]> {
    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const resolvedIds = input.ids.map((id) => mustResolveExisting(state, id, input.exactId));
      const events: EventRecord[] = [];

      for (const id of resolvedIds) {
        const existing = mustTask(state, id);
        if (existing.status === "closed") {
          throw new TsqError("VALIDATION_ERROR", `task ${id} is already closed`, 1);
        }
        if (existing.status === "canceled") {
          throw new TsqError("VALIDATION_ERROR", `cannot close canceled task ${id}`, 1);
        }
        const payload: Record<string, unknown> = { status: "closed" };
        if (input.reason) {
          payload.reason = input.reason;
        }
        events.push(makeEvent(this.actor, this.now(), "task.updated", id, payload));
      }

      const nextState = applyEvents(state, events);
      await appendEvents(this.repoRoot, events);
      await persistProjection(this.repoRoot, nextState, allEvents.length + events.length);
      return resolvedIds.map((id) => mustTask(nextState, id));
    });
  }

  async reopen(input: ReopenInput): Promise<Task[]> {
    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const resolvedIds = input.ids.map((id) => mustResolveExisting(state, id, input.exactId));
      const events: EventRecord[] = [];

      for (const id of resolvedIds) {
        const existing = mustTask(state, id);
        if (existing.status !== "closed") {
          throw new TsqError(
            "VALIDATION_ERROR",
            `cannot reopen task ${id} with status ${existing.status}`,
            1,
          );
        }
        events.push(
          makeEvent(this.actor, this.now(), "task.updated", id, {
            status: "open",
          }),
        );
      }

      const nextState = applyEvents(state, events);
      await appendEvents(this.repoRoot, events);
      await persistProjection(this.repoRoot, nextState, allEvents.length + events.length);
      return resolvedIds.map((id) => mustTask(nextState, id));
    });
  }

  async history(input: HistoryInput): Promise<HistoryResult> {
    const { state, allEvents } = await loadProjectedState(this.repoRoot);
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

  async labelAdd(input: LabelInput): Promise<Task> {
    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const id = mustResolveExisting(state, input.id, input.exactId);
      const existing = mustTask(state, id);
      const newLabels = addLabel(existing.labels, input.label);
      const event = makeEvent(this.actor, this.now(), "task.updated", id, {
        labels: newLabels,
      });
      const nextState = applyEvents(state, [event]);
      await appendEvents(this.repoRoot, [event]);
      await persistProjection(this.repoRoot, nextState, allEvents.length + 1);
      return mustTask(nextState, id);
    });
  }

  async labelRemove(input: LabelInput): Promise<Task> {
    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const id = mustResolveExisting(state, input.id, input.exactId);
      const existing = mustTask(state, id);
      const newLabels = removeLabel(existing.labels, input.label);
      const event = makeEvent(this.actor, this.now(), "task.updated", id, {
        labels: newLabels,
      });
      const nextState = applyEvents(state, [event]);
      await appendEvents(this.repoRoot, [event]);
      await persistProjection(this.repoRoot, nextState, allEvents.length + 1);
      return mustTask(nextState, id);
    });
  }

  async labelList(): Promise<Array<{ label: string; count: number }>> {
    const { state } = await loadProjectedState(this.repoRoot);
    const counts = new Map<string, number>();
    for (const task of Object.values(state.tasks)) {
      for (const label of task.labels) {
        counts.set(label, (counts.get(label) ?? 0) + 1);
      }
    }
    return [...counts.entries()]
      .map(([label, count]) => ({ label, count }))
      .sort((a, b) => a.label.localeCompare(b.label));
  }

  async depTree(input: DepTreeInput): Promise<DepTreeNode> {
    const { state } = await loadProjectedState(this.repoRoot);
    const id = mustResolveExisting(state, input.id, input.exactId);
    return buildDepTree(state, id, input.direction ?? "both", input.depth);
  }

  async search(input: SearchInput): Promise<Task[]> {
    const { state } = await loadProjectedState(this.repoRoot);
    const filter = parseQuery(input.query);
    return sortTasks(evaluateQuery(Object.values(state.tasks), filter, state));
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

function sortStaleTasks(tasks: Task[]): Task[] {
  return [...tasks].sort((a, b) => {
    if (a.updated_at !== b.updated_at) {
      return a.updated_at.localeCompare(b.updated_at);
    }
    if (a.priority !== b.priority) {
      return a.priority - b.priority;
    }
    return a.id.localeCompare(b.id);
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
    if (filter.label && !task.labels.includes(filter.label)) {
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

type SpecAttachSource =
  | { type: "file"; path: string }
  | { type: "stdin" }
  | { type: "text"; content: string };

function resolveSpecAttachSource(input: SpecAttachInput): SpecAttachSource {
  const file = normalizeOptionalInput(input.file);
  const positional = normalizeOptionalInput(input.source);
  const hasStdin = input.stdin === true;
  const hasText = input.text !== undefined;

  const sourcesProvided = [file !== undefined, positional !== undefined, hasStdin, hasText].filter(
    (value) => value,
  ).length;
  if (sourcesProvided !== 1) {
    throw new TsqError(
      "VALIDATION_ERROR",
      "exactly one source is required: --file, --stdin, --text, or positional source path",
      1,
    );
  }

  if (hasText) {
    return { type: "text", content: input.text ?? "" };
  }
  if (hasStdin) {
    return { type: "stdin" };
  }
  return { type: "file", path: file ?? positional ?? "" };
}

async function readSpecAttachContent(source: SpecAttachSource): Promise<string> {
  if (source.type === "text") {
    return source.content;
  }
  if (source.type === "stdin") {
    return readStdinContent();
  }

  try {
    return await readFile(source.path, "utf8");
  } catch (error) {
    throw new TsqError("IO_ERROR", `failed reading spec source file: ${source.path}`, 2, error);
  }
}

async function readStdinContent(): Promise<string> {
  process.stdin.setEncoding("utf8");
  let content = "";
  for await (const chunk of process.stdin) {
    content += chunk;
  }
  return content;
}

async function writeTaskSpecAtomic(
  repoRoot: string,
  taskId: string,
  content: string,
): Promise<{ specPath: string; content: string }> {
  const specFile = taskSpecFile(repoRoot, taskId);
  const specPath = taskSpecRelativePath(taskId);
  await mkdir(dirname(specFile), { recursive: true });
  const temp = `${specFile}.tmp-${process.pid}-${Date.now()}`;

  try {
    const handle = await open(temp, "w");
    try {
      await handle.writeFile(content, "utf8");
      await handle.sync();
    } finally {
      await handle.close();
    }
    await rename(temp, specFile);
    return {
      specPath,
      content: await readFile(specFile, "utf8"),
    };
  } catch (error) {
    throw new TsqError("IO_ERROR", "failed writing attached spec", 2, error);
  }
}

function sha256(content: string): string {
  return createHash("sha256").update(content, "utf8").digest("hex");
}

function normalizeOptionalInput(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
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
