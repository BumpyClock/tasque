import { applyEvents } from "../domain/projector";
import { createEmptyState } from "../domain/state";
import { readConfig } from "../store/config";
import { readEvents } from "../store/events";
import { loadLatestSnapshotWithWarning, writeSnapshot } from "../store/snapshots";
import { readStateCache, writeStateCache } from "../store/state";
import type { EventRecord, Snapshot, State } from "../types";

export interface LoadedState {
  state: State;
  allEvents: EventRecord[];
  warning?: string;
  snapshot: Snapshot | null;
}

export async function loadProjectedState(repoRoot: string): Promise<LoadedState> {
  const { events, warning: eventWarning } = await readEvents(repoRoot);
  const fromCache = await readStateCache(repoRoot);
  if (fromCache && fromCache.applied_events <= events.length) {
    const offset = fromCache.applied_events ?? 0;
    if (offset === events.length) {
      return {
        state: fromCache,
        allEvents: events,
        warning: eventWarning,
        snapshot: null,
      };
    }

    const state = applyEvents(fromCache, events.slice(offset));
    state.applied_events = events.length;
    return {
      state,
      allEvents: events,
      warning: eventWarning,
      snapshot: null,
    };
  }

  const { snapshot, warning: snapshotWarning } = await loadLatestSnapshotWithWarning(repoRoot);
  const base = snapshot ? snapshot.state : createEmptyState();
  const startOffset = snapshot ? Math.min(snapshot.event_count, events.length) : 0;
  const projected = applyEvents(base, events.slice(startOffset));
  projected.applied_events = events.length;

  return {
    state: projected,
    allEvents: events,
    warning: combineWarnings(eventWarning, snapshotWarning),
    snapshot,
  };
}

export async function persistProjection(
  repoRoot: string,
  state: State,
  eventCount: number,
): Promise<void> {
  state.applied_events = eventCount;
  await writeStateCache(repoRoot, state);

  const config = await readConfig(repoRoot);
  if (config.snapshot_every <= 0) {
    return;
  }

  if (eventCount > 0 && eventCount % config.snapshot_every === 0) {
    await writeSnapshot(repoRoot, {
      taken_at: new Date().toISOString(),
      event_count: eventCount,
      state,
    });
  }
}

function combineWarnings(...warnings: Array<string | undefined>): string | undefined {
  const present = warnings.filter((warning): warning is string =>
    Boolean(warning && warning.length > 0),
  );
  if (present.length === 0) {
    return undefined;
  }
  return present.join(" | ");
}
