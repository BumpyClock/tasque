import { readdir, unlink } from "node:fs/promises";
import { join } from "node:path";
import { ulid } from "ulid";
import { applyEvents } from "../domain/projector";
import { TsqError } from "../errors";
import { appendEvents } from "../store/events";
import { forceRemoveLock, lockExists, withWriteLock } from "../store/lock";
import { getPaths } from "../store/paths";
import { SNAPSHOT_RETAIN_COUNT } from "../store/snapshots";
import type {
  EventPayloadMap,
  EventRecord,
  EventType,
  RelationType,
  RepairPlan,
  RepairResult,
  State,
} from "../types";
import { loadProjectedState, persistProjection } from "./state";

function makeRepairEvent<T extends EventType>(
  actor: string,
  ts: string,
  type: T,
  taskId: string,
  payload: EventPayloadMap[T],
): EventRecord {
  return { event_id: ulid(), ts, actor, type, task_id: taskId, payload: payload as Record<string, unknown> };
}

function buildRepairPlan(
  state: State,
): Omit<RepairPlan, "stale_temps" | "stale_lock" | "old_snapshots"> {
  const orphaned_deps: RepairPlan["orphaned_deps"] = [];
  const orphaned_links: RepairPlan["orphaned_links"] = [];

  for (const [child, blockers] of Object.entries(state.deps)) {
    for (const blocker of blockers) {
      if (!state.tasks[child] || !state.tasks[blocker]) {
        orphaned_deps.push({ child, blocker });
      }
    }
  }

  for (const [src, rels] of Object.entries(state.links)) {
    for (const [kind, targets] of Object.entries(rels)) {
      for (const target of targets ?? []) {
        if (!state.tasks[src] || !state.tasks[target]) {
          orphaned_links.push({ src, dst: target, type: kind as RelationType });
        }
      }
    }
  }

  return { orphaned_deps, orphaned_links };
}

async function scanFilesystem(
  repoRoot: string,
): Promise<{ stale_temps: string[]; stale_lock: boolean; old_snapshots: string[] }> {
  const paths = getPaths(repoRoot);
  const stale_temps: string[] = [];
  let stale_lock = false;
  let old_snapshots: string[] = [];

  try {
    const entries = await readdir(paths.tasqueDir);
    for (const entry of entries) {
      if (entry.includes(".tmp")) {
        stale_temps.push(entry);
      }
    }
  } catch {}

  stale_lock = await lockExists(repoRoot);

  try {
    const snapEntries = await readdir(paths.snapshotsDir);
    const snapshots = snapEntries.filter((name) => name.endsWith(".json")).sort();
    if (snapshots.length > SNAPSHOT_RETAIN_COUNT) {
      old_snapshots = snapshots.slice(0, snapshots.length - SNAPSHOT_RETAIN_COUNT);
    }
  } catch {}

  return { stale_temps, stale_lock, old_snapshots };
}

export async function executeRepair(
  repoRoot: string,
  actor: string,
  now: () => string,
  opts: { fix: boolean; forceUnlock: boolean },
): Promise<RepairResult> {
  if (opts.forceUnlock && !opts.fix) {
    throw new TsqError("VALIDATION_ERROR", "--force-unlock requires --fix", 1);
  }

  const { state } = await loadProjectedState(repoRoot);
  const { orphaned_deps, orphaned_links } = buildRepairPlan(state);
  const { stale_temps, stale_lock, old_snapshots } = await scanFilesystem(repoRoot);
  const plan: RepairPlan = {
    orphaned_deps,
    orphaned_links,
    stale_temps,
    stale_lock,
    old_snapshots,
  };

  if (!opts.fix) {
    return { plan, applied: false, events_appended: 0, files_removed: 0 };
  }

  if (opts.forceUnlock && plan.stale_lock) {
    await forceRemoveLock(repoRoot);
  }

  return withWriteLock(repoRoot, async () => {
    const { state: lockedState, allEvents } = await loadProjectedState(repoRoot);
    const events: EventRecord[] = [];
    const paths = getPaths(repoRoot);

    for (const dep of plan.orphaned_deps) {
      events.push(
        makeRepairEvent(actor, now(), "dep.removed", dep.child, { blocker: dep.blocker }),
      );
    }

    for (const link of plan.orphaned_links) {
      events.push(
        makeRepairEvent(actor, now(), "link.removed", link.src, {
          type: link.type,
          target: link.dst,
        }),
      );
    }

    if (events.length > 0) {
      const nextState = applyEvents(lockedState, events);
      await appendEvents(repoRoot, events);
      await persistProjection(repoRoot, nextState, allEvents.length + events.length);
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

    return { plan, applied: true, events_appended: events.length, files_removed: filesRemoved };
  });
}
