import { mkdir, open, readFile } from "node:fs/promises";

import { TsqError } from "../errors";
import type { EventRecord, EventType } from "../types";
import { getPaths } from "./paths";

/**
 * Required payload fields per event type. Used for runtime validation when
 * reading events from JSONL. Each entry lists the field names that must be
 * present and the expected typeof for each.
 */
const PAYLOAD_REQUIRED_FIELDS: Record<EventType, Array<{ field: string; type: string }>> = {
  "task.created": [
    { field: "title", type: "string" },
  ],
  "task.updated": [],
  "task.claimed": [],
  "task.noted": [
    { field: "text", type: "string" },
  ],
  "task.spec_attached": [
    { field: "spec_path", type: "string" },
    { field: "spec_fingerprint", type: "string" },
  ],
  "task.superseded": [],
  "dep.added": [
    { field: "blocker", type: "string" },
  ],
  "dep.removed": [
    { field: "blocker", type: "string" },
  ],
  "link.added": [
    { field: "type", type: "string" },
  ],
  "link.removed": [
    { field: "type", type: "string" },
  ],
};

const VALID_EVENT_TYPES = new Set<string>(Object.keys(PAYLOAD_REQUIRED_FIELDS));

/**
 * Validates that an event's payload contains the required fields for its type.
 * Throws `TsqError` with code `EVENTS_CORRUPT` if validation fails.
 */
function validateEventPayload(
  raw: Record<string, unknown>,
  lineNumber: number,
): void {
  const eventType = raw.type as string;
  if (!VALID_EVENT_TYPES.has(eventType)) {
    throw new TsqError(
      "EVENTS_CORRUPT",
      `Invalid event at line ${lineNumber}: unknown event type "${eventType}"`,
      2,
      { line: lineNumber, type: eventType },
    );
  }

  const payload = raw.payload;
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
    throw new TsqError(
      "EVENTS_CORRUPT",
      `Invalid event at line ${lineNumber}: payload must be an object`,
      2,
      { line: lineNumber, event_id: raw.event_id },
    );
  }

  const payloadObj = payload as Record<string, unknown>;
  const requiredFields = PAYLOAD_REQUIRED_FIELDS[eventType as EventType];
  for (const { field, type: expectedType } of requiredFields) {
    const value = payloadObj[field];
    if (value === undefined || value === null || typeof value !== expectedType) {
      throw new TsqError(
        "EVENTS_CORRUPT",
        `Invalid event at line ${lineNumber}: ${eventType} payload missing required field "${field}" (expected ${expectedType})`,
        2,
        { line: lineNumber, event_id: raw.event_id, field, expected_type: expectedType },
      );
    }
  }
}

export async function appendEvents(repoRoot: string, events: EventRecord[]): Promise<void> {
  if (events.length === 0) {
    return;
  }

  const paths = getPaths(repoRoot);
  await mkdir(paths.tasqueDir, { recursive: true });

  const payload = `${events.map((event) => JSON.stringify(event)).join("\n")}\n`;

  try {
    const handle = await open(paths.eventsFile, "a");
    try {
      await handle.writeFile(payload, "utf8");
      await handle.sync();
    } finally {
      await handle.close();
    }
  } catch (error) {
    throw new TsqError("EVENT_APPEND_FAILED", "Failed appending events", 2, error);
  }
}

export async function readEvents(
  repoRoot: string,
): Promise<{ events: EventRecord[]; warning?: string }> {
  const paths = getPaths(repoRoot);

  let raw: string;
  try {
    raw = await readFile(paths.eventsFile, "utf8");
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "ENOENT") {
      return { events: [] };
    }
    throw new TsqError("EVENT_READ_FAILED", "Failed reading events", 2, error);
  }

  const lines = raw.split(/\r?\n/);
  if (lines.at(-1) === "") {
    lines.pop();
  }

  const events: EventRecord[] = [];
  let warning: string | undefined;

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (!line || line.trim().length === 0) {
      continue;
    }

    try {
      const raw = JSON.parse(line) as Record<string, unknown>;
      const REQUIRED_FIELDS = ["event_id", "ts", "actor", "type", "task_id"] as const;
      for (const field of REQUIRED_FIELDS) {
        if (typeof raw[field] !== "string") {
          throw new TsqError(
            "EVENTS_CORRUPT",
            `Invalid event at line ${index + 1}: missing or non-string field "${field}"`,
            2,
            { line: index + 1, field },
          );
        }
      }
      validateEventPayload(raw, index + 1);
      events.push(raw as unknown as EventRecord);
    } catch (error) {
      if (error instanceof TsqError) {
        throw error;
      }
      if (index === lines.length - 1) {
        warning = `Ignored malformed trailing JSONL line in ${paths.eventsFile}`;
        break;
      }
      throw new TsqError("EVENTS_CORRUPT", `Malformed events JSONL at line ${index + 1}`, 2, {
        line: index + 1,
        error,
      });
    }
  }

  if (warning) {
    return { events, warning };
  }

  return { events };
}
