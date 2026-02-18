import { ulid } from "ulid";
import type { EventPayloadMap, EventRecord, EventType } from "../types";

/** Shared event factory for constructing EventRecord instances. */
export function makeEvent<T extends EventType>(
  actor: string,
  ts: string,
  type: T,
  taskId: string,
  payload: EventPayloadMap[T],
): EventRecord {
  const id = ulid();
  return {
    id,
    event_id: id,
    ts,
    actor,
    type,
    task_id: taskId,
    payload: payload as Record<string, unknown>,
  };
}
