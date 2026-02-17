import { applyEvents } from "../domain/projector";
import { createEmptyState } from "../domain/state";
import { readConfig } from "../store/config";
import { readEvents } from "../store/events";
import { loadLatestSnapshot, writeSnapshot } from "../store/snapshots";
import { readStateCache, writeStateCache } from "../store/state";
import type { EventRecord, Snapshot, State } from "../types";

export interface LoadedState {
  state: State;
  allEvents: EventRecord[];
  warning?: string;
  snapshot: Snapshot | null;
}

export async function loadProjectedState(repoRoot: string): Promise<LoadedState> {
  const { events, warning } = await readEvents(repoRoot);
  const fromCache = await readStateCache(repoRoot);
  if (fromCache && fromCache.applied_events <= events.length) {
    const offset = fromCache.applied_events ?? 0;
    if (offset === events.length) {
      return {
        state: fromCache,
        allEvents: events,
        warning,
        snapshot: null,
      };
    }

    const state = applyEvents(fromCache, events.slice(offset));
    state.applied_events = events.length;
    return {
      state,
      allEvents: events,
      warning,
      snapshot: null,
    };
  }

  const snapshot = await loadLatestSnapshot(repoRoot);
  const base = snapshot ? snapshot.state : createEmptyState();
  const startOffset = snapshot ? Math.min(snapshot.event_count, events.length) : 0;
  const projected = applyEvents(base, events.slice(startOffset));
  projected.applied_events = events.length;

  return {
    state: projected,
    allEvents: events,
    warning,
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
