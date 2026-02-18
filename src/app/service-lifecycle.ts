import { makeEvent } from "../domain/events";
import { applyEvents } from "../domain/projector";
import { assertNoDependencyCycle } from "../domain/validate";
import { TsqError } from "../errors";
import type { EventRecord, RelationType, Task } from "../types";
import type {
  ClaimInput,
  CloseInput,
  DepInput,
  DuplicateCandidatesResult,
  DuplicateInput,
  LinkInput,
  MergeInput,
  MergeResult,
  ReopenInput,
  ServiceContext,
  SupersedeInput,
} from "./service-types";
import {
  createsDuplicateCycle,
  hasDuplicateLink,
  mustResolveExisting,
  mustTask,
  normalizeDuplicateTitle,
  sortTasks,
} from "./service-utils";
import {
  appendEvents,
  evaluateTaskSpec,
  loadProjectedState,
  persistProjection,
  withWriteLock,
} from "./storage";

export async function claim(ctx: ServiceContext, input: ClaimInput): Promise<Task> {
  return withWriteLock(ctx.repoRoot, async () => {
    const { state, allEvents } = await loadProjectedState(ctx.repoRoot);
    const id = mustResolveExisting(state, input.id, input.exactId);
    const existing = mustTask(state, id);
    const CLAIMABLE_STATUSES = ["open", "in_progress"];
    if (!CLAIMABLE_STATUSES.includes(existing.status)) {
      throw new TsqError("INVALID_STATUS", `cannot claim task with status '${existing.status}'`, 1);
    }
    if (existing.assignee) {
      throw new TsqError("CLAIM_CONFLICT", `task already assigned to ${existing.assignee}`, 1);
    }
    if (input.requireSpec) {
      const specCheck = await evaluateTaskSpec(ctx.repoRoot, id, existing);
      if (!specCheck.ok) {
        throw new TsqError(
          "SPEC_VALIDATION_FAILED",
          "cannot claim task because required spec check failed",
          1,
          {
            task_id: id,
            diagnostics: specCheck.diagnostics,
          },
        );
      }
    }
    const assignee = input.assignee ?? ctx.actor;
    const event = makeEvent(ctx.actor, ctx.now(), "task.claimed", id, {
      assignee,
    });
    const nextState = applyEvents(state, [event]);
    await appendEvents(ctx.repoRoot, [event]);
    await persistProjection(ctx.repoRoot, nextState, allEvents.length + 1);
    return mustTask(nextState, id);
  });
}

export async function close(ctx: ServiceContext, input: CloseInput): Promise<Task[]> {
  return withWriteLock(ctx.repoRoot, async () => {
    const { state, allEvents } = await loadProjectedState(ctx.repoRoot);
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
      const ts = ctx.now();
      events.push(
        makeEvent(ctx.actor, ts, "task.status_set", id, {
          status: "closed",
          closed_at: ts,
          reason: input.reason,
        }),
      );
    }

    const nextState = applyEvents(state, events);
    await appendEvents(ctx.repoRoot, events);
    await persistProjection(ctx.repoRoot, nextState, allEvents.length + events.length);
    return resolvedIds.map((id) => mustTask(nextState, id));
  });
}

export async function reopen(ctx: ServiceContext, input: ReopenInput): Promise<Task[]> {
  return withWriteLock(ctx.repoRoot, async () => {
    const { state, allEvents } = await loadProjectedState(ctx.repoRoot);
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
        makeEvent(ctx.actor, ctx.now(), "task.status_set", id, {
          status: "open",
        }),
      );
    }

    const nextState = applyEvents(state, events);
    await appendEvents(ctx.repoRoot, events);
    await persistProjection(ctx.repoRoot, nextState, allEvents.length + events.length);
    return resolvedIds.map((id) => mustTask(nextState, id));
  });
}

export async function supersede(ctx: ServiceContext, input: SupersedeInput): Promise<Task> {
  return withWriteLock(ctx.repoRoot, async () => {
    const { state, allEvents } = await loadProjectedState(ctx.repoRoot);
    const source = mustResolveExisting(state, input.source, input.exactId);
    const withId = mustResolveExisting(state, input.withId, input.exactId);
    if (source === withId) {
      throw new TsqError("VALIDATION_ERROR", "cannot supersede task with itself", 1);
    }
    const event = makeEvent(ctx.actor, ctx.now(), "task.superseded", source, {
      with: withId,
      reason: input.reason,
    });
    const nextState = applyEvents(state, [event]);
    await appendEvents(ctx.repoRoot, [event]);
    await persistProjection(ctx.repoRoot, nextState, allEvents.length + 1);
    return mustTask(nextState, source);
  });
}

export async function duplicate(ctx: ServiceContext, input: DuplicateInput): Promise<Task> {
  return withWriteLock(ctx.repoRoot, async () => {
    const { state, allEvents } = await loadProjectedState(ctx.repoRoot);
    const source = mustResolveExisting(state, input.source, input.exactId);
    const canonical = mustResolveExisting(state, input.canonical, input.exactId);
    if (source === canonical) {
      throw new TsqError("VALIDATION_ERROR", "cannot mark task as duplicate of itself", 1);
    }

    const sourceTask = mustTask(state, source);
    const canonicalTask = mustTask(state, canonical);
    if (sourceTask.status === "canceled") {
      throw new TsqError("INVALID_STATUS", `cannot duplicate canceled task ${source}`, 1);
    }
    if (canonicalTask.status === "canceled") {
      throw new TsqError("INVALID_STATUS", `cannot use canceled canonical task ${canonical}`, 1);
    }
    if (sourceTask.duplicate_of && sourceTask.duplicate_of !== canonical) {
      throw new TsqError(
        "VALIDATION_ERROR",
        `task ${source} is already marked as duplicate of ${sourceTask.duplicate_of}`,
        1,
      );
    }
    if (createsDuplicateCycle(state, source, canonical)) {
      throw new TsqError(
        "DUPLICATE_CYCLE",
        `duplicate cycle detected: ${source} -> ${canonical}`,
        1,
      );
    }

    const events: EventRecord[] = [];
    if (!hasDuplicateLink(state, source, canonical)) {
      events.push(
        makeEvent(ctx.actor, ctx.now(), "link.added", source, {
          type: "duplicates",
          target: canonical,
        }),
      );
    }

    events.push(
      makeEvent(ctx.actor, ctx.now(), "task.updated", source, {
        duplicate_of: canonical,
      }),
    );
    const ts = ctx.now();
    events.push(
      makeEvent(ctx.actor, ts, "task.status_set", source, {
        status: "closed",
        closed_at: ts,
        reason: input.reason,
      }),
    );

    const nextState = applyEvents(state, events);
    await appendEvents(ctx.repoRoot, events);
    await persistProjection(ctx.repoRoot, nextState, allEvents.length + events.length);
    return mustTask(nextState, source);
  });
}

export async function merge(ctx: ServiceContext, input: MergeInput): Promise<MergeResult> {
  if (input.sources.length === 0) {
    throw new TsqError("VALIDATION_ERROR", "at least one source task is required", 1);
  }

  return withWriteLock(ctx.repoRoot, async () => {
    const { state, allEvents } = await loadProjectedState(ctx.repoRoot);

    const targetId = mustResolveExisting(state, input.into, input.exactId);
    const targetTask = mustTask(state, targetId);
    const warnings: string[] = [];

    if ((targetTask.status === "closed" || targetTask.status === "canceled") && !input.force) {
      throw new TsqError(
        "VALIDATION_ERROR",
        `target task ${targetId} is ${targetTask.status}; use --force to merge anyway`,
        1,
      );
    }
    if ((targetTask.status === "closed" || targetTask.status === "canceled") && input.force) {
      warnings.push(`target ${targetId} is ${targetTask.status} (forced)`);
    }

    const resolvedSources = input.sources.map((s) => mustResolveExisting(state, s, input.exactId));

    for (const src of resolvedSources) {
      if (src === targetId) {
        throw new TsqError("VALIDATION_ERROR", `source ${src} cannot be the same as target`, 1);
      }
    }

    const events: EventRecord[] = [];
    const merged: Array<{ id: string; status: string }> = [];

    for (const sourceId of resolvedSources) {
      const sourceTask = mustTask(state, sourceId);

      if (sourceTask.status === "closed" || sourceTask.status === "canceled") {
        warnings.push(`${sourceId} already ${sourceTask.status}, skipped`);
        continue;
      }

      if (createsDuplicateCycle(state, sourceId, targetId)) {
        warnings.push(`${sourceId} -> ${targetId} would create a cycle, skipped`);
        continue;
      }

      if (!hasDuplicateLink(state, sourceId, targetId)) {
        events.push(
          makeEvent(ctx.actor, ctx.now(), "link.added", sourceId, {
            type: "duplicates",
            target: targetId,
          }),
        );
      }

      events.push(
        makeEvent(ctx.actor, ctx.now(), "task.updated", sourceId, {
          duplicate_of: targetId,
        }),
      );

      const ts = ctx.now();
      events.push(
        makeEvent(ctx.actor, ts, "task.status_set", sourceId, {
          status: "closed",
          closed_at: ts,
          reason: input.reason,
        }),
      );

      merged.push({ id: sourceId, status: "closed" });
    }

    if (input.dryRun) {
      // Apply events to get projected state for the result, but don't persist
      const projectedState = events.length > 0 ? applyEvents(state, events) : state;
      const projTarget = mustTask(projectedState, targetId);
      const projectedSources = resolvedSources.map((id) => mustTask(projectedState, id));
      return {
        merged,
        target: { id: targetId, title: projTarget.title, status: projTarget.status },
        dry_run: true,
        warnings,
        plan_summary: {
          requested_sources: resolvedSources.length,
          merged_sources: merged.length,
          skipped_sources: resolvedSources.length - merged.length,
          planned_events: events.length,
        },
        projected: {
          target: projTarget,
          sources: projectedSources,
        },
      };
    }

    if (events.length > 0) {
      const nextState = applyEvents(state, events);
      await appendEvents(ctx.repoRoot, events);
      await persistProjection(ctx.repoRoot, nextState, allEvents.length + events.length);
      const finalTarget = mustTask(nextState, targetId);
      return {
        merged,
        target: { id: targetId, title: finalTarget.title, status: finalTarget.status },
        dry_run: false,
        warnings,
      };
    }

    return {
      merged,
      target: { id: targetId, title: targetTask.title, status: targetTask.status },
      dry_run: false,
      warnings,
    };
  });
}

export async function duplicateCandidates(
  ctx: ServiceContext,
  limit = 20,
): Promise<DuplicateCandidatesResult> {
  if (!Number.isInteger(limit) || limit <= 0 || limit > 200) {
    throw new TsqError("VALIDATION_ERROR", "limit must be an integer between 1 and 200", 1);
  }

  const { state } = await loadProjectedState(ctx.repoRoot);
  const candidates = Object.values(state.tasks).filter((task) => {
    if (task.status === "closed" || task.status === "canceled") {
      return false;
    }
    return !task.duplicate_of;
  });

  const groups = new Map<string, Task[]>();
  for (const task of candidates) {
    const key = normalizeDuplicateTitle(task.title);
    if (key.length < 4) {
      continue;
    }
    const grouped = groups.get(key);
    if (grouped) {
      grouped.push(task);
    } else {
      groups.set(key, [task]);
    }
  }

  const grouped = [...groups.entries()]
    .filter(([, tasks]) => tasks.length > 1)
    .sort(([a], [b]) => a.localeCompare(b))
    .slice(0, limit)
    .map(([key, tasks]) => ({
      key,
      tasks: sortTasks(tasks),
    }));

  return {
    scanned: candidates.length,
    groups: grouped,
  };
}

export async function depAdd(
  ctx: ServiceContext,
  input: DepInput,
): Promise<{ child: string; blocker: string; dep_type: "blocks" | "starts_after" }> {
  return withWriteLock(ctx.repoRoot, async () => {
    const { state, allEvents } = await loadProjectedState(ctx.repoRoot);
    const child = mustResolveExisting(state, input.child, input.exactId);
    const blocker = mustResolveExisting(state, input.blocker, input.exactId);
    const depType = input.depType ?? "blocks";
    if (child === blocker) {
      throw new TsqError("VALIDATION_ERROR", "task cannot depend on itself", 1);
    }
    if (depType === "blocks") {
      assertNoDependencyCycle(state, child, blocker);
    }
    const event = makeEvent(ctx.actor, ctx.now(), "dep.added", child, {
      blocker,
      dep_type: depType,
    });
    const nextState = applyEvents(state, [event]);
    await appendEvents(ctx.repoRoot, [event]);
    await persistProjection(ctx.repoRoot, nextState, allEvents.length + 1);
    return { child, blocker, dep_type: depType };
  });
}

export async function depRemove(
  ctx: ServiceContext,
  input: DepInput,
): Promise<{ child: string; blocker: string; dep_type: "blocks" | "starts_after" }> {
  return withWriteLock(ctx.repoRoot, async () => {
    const { state, allEvents } = await loadProjectedState(ctx.repoRoot);
    const child = mustResolveExisting(state, input.child, input.exactId);
    const blocker = mustResolveExisting(state, input.blocker, input.exactId);
    const depType = input.depType ?? "blocks";
    const event = makeEvent(ctx.actor, ctx.now(), "dep.removed", child, {
      blocker,
      dep_type: depType,
    });
    const nextState = applyEvents(state, [event]);
    await appendEvents(ctx.repoRoot, [event]);
    await persistProjection(ctx.repoRoot, nextState, allEvents.length + 1);
    return { child, blocker, dep_type: depType };
  });
}

export async function linkAdd(
  ctx: ServiceContext,
  input: LinkInput,
): Promise<{ src: string; dst: string; type: RelationType }> {
  return withWriteLock(ctx.repoRoot, async () => {
    const { state, allEvents } = await loadProjectedState(ctx.repoRoot);
    const src = mustResolveExisting(state, input.src, input.exactId);
    const dst = mustResolveExisting(state, input.dst, input.exactId);
    if (src === dst) {
      throw new TsqError("VALIDATION_ERROR", "self-edge not allowed", 1);
    }
    const event = makeEvent(ctx.actor, ctx.now(), "link.added", src, {
      type: input.type,
      target: dst,
    });
    const nextState = applyEvents(state, [event]);
    await appendEvents(ctx.repoRoot, [event]);
    await persistProjection(ctx.repoRoot, nextState, allEvents.length + 1);
    return { src, dst, type: input.type };
  });
}

export async function linkRemove(
  ctx: ServiceContext,
  input: LinkInput,
): Promise<{ src: string; dst: string; type: RelationType }> {
  return withWriteLock(ctx.repoRoot, async () => {
    const { state, allEvents } = await loadProjectedState(ctx.repoRoot);
    const src = mustResolveExisting(state, input.src, input.exactId);
    const dst = mustResolveExisting(state, input.dst, input.exactId);
    if (src === dst) {
      throw new TsqError("VALIDATION_ERROR", "self-edge not allowed", 1);
    }
    const event = makeEvent(ctx.actor, ctx.now(), "link.removed", src, {
      type: input.type,
      target: dst,
    });
    const nextState = applyEvents(state, [event]);
    await appendEvents(ctx.repoRoot, [event]);
    await persistProjection(ctx.repoRoot, nextState, allEvents.length + 1);
    return { src, dst, type: input.type };
  });
}
