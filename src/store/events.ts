import { mkdir, open, readFile } from "node:fs/promises";

import { TsqError } from "../errors";
import type { EventRecord } from "../types";
import { getPaths } from "./paths";

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
      const parsed = JSON.parse(line) as EventRecord;
      events.push(parsed);
    } catch (error) {
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
