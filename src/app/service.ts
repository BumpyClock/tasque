import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { buildDepTree } from "../domain/dep-tree";
import { makeEvent } from "../domain/events";
import { nextChildId } from "../domain/ids";
import { addLabel, removeLabel } from "../domain/labels";
import { applyEvents } from "../domain/projector";
import { TsqError } from "../errors";
import { applySkillOperation } from "../skills";
import type { EventRecord, RepairResult, Task, TaskStatus, TaskUpdatedPayload } from "../types";
import { executeRepair } from "./repair";
import * as lifecycle from "./service-lifecycle";
import * as query from "./service-query";
import type { ServiceContext } from "./service-types";
import { mustResolveExisting, mustTask, uniqueRootId } from "./service-utils";
import {
  appendEvents,
  ensureEventsFile,
  ensureTasqueGitignore,
  evaluateTaskSpec,
  loadProjectedState,
  normalizeOptionalInput,
  persistProjection,
  readSpecAttachContent,
  resolveSpecAttachSource,
  sha256,
  withWriteLock,
  writeDefaultConfig,
  writeTaskSpecAtomic,
} from "./storage";

// Re-export all types so existing importers continue to work
export type {
  ClaimInput,
  CloseInput,
  CreateInput,
  DepInput,
  DepTreeInput,
  DoctorResult,
  DuplicateCandidatesResult,
  DuplicateInput,
  HistoryInput,
  HistoryResult,
  InitInput,
  InitResult,
  LabelInput,
  LinkInput,
  ListFilter,
  NoteAddInput,
  NoteAddResult,
  NoteListInput,
  NoteListResult,
  ReopenInput,
  SearchInput,
  SpecAttachInput,
  SpecAttachResult,
  SpecCheckDiagnostic,
  SpecCheckInput,
  SpecCheckResult,
  StaleInput,
  StaleResult,
  SupersedeInput,
  UpdateInput,
} from "./service-types";

/**
 * Core service orchestrating business logic for task management.
 *
 * Usage:
 *   const svc = new TasqueService(repoRoot, "user", () => new Date().toISOString());
 *   const task = await svc.create({ title: "Fix bug", kind: "task", priority: 1 });
 */
export class TasqueService {
  private readonly ctx: ServiceContext;

  constructor(
    private readonly repoRoot: string,
    private readonly actor: string,
    private readonly now: () => string,
  ) {
    this.ctx = { repoRoot, actor, now };
  }

  async init(
    input: import("./service-types").InitInput = {},
  ): Promise<import("./service-types").InitResult> {
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

  async create(input: import("./service-types").CreateInput): Promise<Task> {
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
        external_ref: input.externalRef,
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

  async show(idRaw: string, exactId?: boolean) {
    return query.show(this.ctx, idRaw, exactId);
  }

  async list(filter: import("./service-types").ListFilter): Promise<Task[]> {
    return query.list(this.ctx, filter);
  }

  async stale(input: import("./service-types").StaleInput) {
    return query.stale(this.ctx, input);
  }

  async listTree(filter: import("./service-types").ListFilter) {
    return query.listTree(this.ctx, filter);
  }

  async ready(): Promise<Task[]> {
    return query.ready(this.ctx);
  }

  async doctor() {
    return query.doctor(this.ctx);
  }

  async update(input: import("./service-types").UpdateInput): Promise<Task> {
    if (input.description !== undefined && input.clearDescription) {
      throw new TsqError(
        "VALIDATION_ERROR",
        "cannot combine --description with --clear-description",
        1,
      );
    }
    if (input.externalRef !== undefined && input.clearExternalRef) {
      throw new TsqError(
        "VALIDATION_ERROR",
        "cannot combine --external-ref with --clear-external-ref",
        1,
      );
    }

    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const id = mustResolveExisting(state, input.id, input.exactId);
      const existing = mustTask(state, id);
      const patch: TaskUpdatedPayload = {};

      if (input.title !== undefined) {
        patch.title = input.title;
      }
      if (input.priority !== undefined) {
        patch.priority = input.priority;
      }
      if (input.description !== undefined) {
        patch.description = input.description;
      }
      if (input.clearDescription) {
        patch.clear_description = true;
      }
      if (input.externalRef !== undefined) {
        patch.external_ref = input.externalRef;
      }
      if (input.clearExternalRef) {
        patch.clear_external_ref = true;
      }

      const hasFieldPatch = Object.keys(patch).length > 0;
      const hasStatusChange = input.status !== undefined;

      if (!hasFieldPatch && !hasStatusChange) {
        throw new TsqError("VALIDATION_ERROR", "no update fields provided", 1);
      }

      if (existing.status === "canceled" && input.status === "in_progress") {
        throw new TsqError("VALIDATION_ERROR", "cannot move canceled task to in_progress", 1);
      }

      const events: EventRecord[] = [];
      if (hasFieldPatch) {
        events.push(makeEvent(this.actor, this.now(), "task.updated", id, patch));
      }
      if (hasStatusChange) {
        const ts = this.now();
        events.push(
          makeEvent(this.actor, ts, "task.status_set", id, {
            status: input.status as TaskStatus,
            closed_at: input.status === "closed" ? ts : undefined,
          }),
        );
      }

      const nextState = applyEvents(state, events);
      await appendEvents(this.repoRoot, events);
      await persistProjection(this.repoRoot, nextState, allEvents.length + events.length);
      return mustTask(nextState, id);
    });
  }

  async noteAdd(input: import("./service-types").NoteAddInput) {
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

  async noteList(input: import("./service-types").NoteListInput) {
    const { state } = await loadProjectedState(this.repoRoot);
    const id = mustResolveExisting(state, input.id, input.exactId);
    const task = mustTask(state, id);
    return {
      task_id: id,
      notes: [...(task.notes ?? [])],
    };
  }

  async specAttach(input: import("./service-types").SpecAttachInput) {
    const source = resolveSpecAttachSource(input);
    const sourceContent = await readSpecAttachContent(source);
    if (sourceContent.trim().length === 0) {
      throw new TsqError("VALIDATION_ERROR", "spec markdown content must not be empty", 1);
    }

    return withWriteLock(this.repoRoot, async () => {
      const { state, allEvents } = await loadProjectedState(this.repoRoot);
      const id = mustResolveExisting(state, input.id, input.exactId);
      const existing = mustTask(state, id);
      const newFingerprint = sha256(sourceContent);
      const oldFingerprint = normalizeOptionalInput(existing.spec_fingerprint);

      if (oldFingerprint && oldFingerprint !== newFingerprint && !input.force) {
        throw new TsqError(
          "SPEC_CONFLICT",
          `task ${id} already has an attached spec with a different fingerprint`,
          1,
          { task_id: id, old_fingerprint: oldFingerprint, new_fingerprint: newFingerprint },
        );
      }

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

  async specCheck(input: import("./service-types").SpecCheckInput) {
    const { state } = await loadProjectedState(this.repoRoot);
    const id = mustResolveExisting(state, input.id, input.exactId);
    const task = mustTask(state, id);
    return evaluateTaskSpec(this.repoRoot, id, task);
  }

  async claim(input: import("./service-types").ClaimInput): Promise<Task> {
    return lifecycle.claim(this.ctx, input);
  }

  async depAdd(input: import("./service-types").DepInput) {
    return lifecycle.depAdd(this.ctx, input);
  }

  async depRemove(input: import("./service-types").DepInput) {
    return lifecycle.depRemove(this.ctx, input);
  }

  async linkAdd(input: import("./service-types").LinkInput) {
    return lifecycle.linkAdd(this.ctx, input);
  }

  async linkRemove(input: import("./service-types").LinkInput) {
    return lifecycle.linkRemove(this.ctx, input);
  }

  async supersede(input: import("./service-types").SupersedeInput): Promise<Task> {
    return lifecycle.supersede(this.ctx, input);
  }

  async duplicate(input: import("./service-types").DuplicateInput): Promise<Task> {
    return lifecycle.duplicate(this.ctx, input);
  }

  async duplicateCandidates(limit = 20) {
    return lifecycle.duplicateCandidates(this.ctx, limit);
  }

  async repair(opts: { fix: boolean; forceUnlock: boolean }): Promise<RepairResult> {
    return executeRepair(this.repoRoot, this.actor, this.now, opts);
  }

  async close(input: import("./service-types").CloseInput): Promise<Task[]> {
    return lifecycle.close(this.ctx, input);
  }

  async reopen(input: import("./service-types").ReopenInput): Promise<Task[]> {
    return lifecycle.reopen(this.ctx, input);
  }

  async history(input: import("./service-types").HistoryInput) {
    return query.history(this.ctx, input);
  }

  async labelAdd(input: import("./service-types").LabelInput): Promise<Task> {
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

  async labelRemove(input: import("./service-types").LabelInput): Promise<Task> {
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

  async depTree(input: import("./service-types").DepTreeInput) {
    const { state } = await loadProjectedState(this.repoRoot);
    const id = mustResolveExisting(state, input.id, input.exactId);
    return buildDepTree(state, id, input.direction ?? "both", input.depth);
  }

  async search(input: import("./service-types").SearchInput): Promise<Task[]> {
    return query.search(this.ctx, input);
  }
}
