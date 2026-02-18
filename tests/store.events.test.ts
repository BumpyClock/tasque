import { afterEach, describe, expect, it } from "bun:test";
import { appendFile, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { TsqError } from "../src/errors";
import { appendEvents, readEvents } from "../src/store/events";
import { getPaths } from "../src/store/paths";
import type { EventRecord } from "../src/types";

const repos: string[] = [];

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-store-events-"));
  repos.push(repo);
  return repo;
}

afterEach(async () => {
  await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
});

/** Minimal payload per event type to satisfy runtime validation. */
const MINIMAL_PAYLOADS: Record<EventRecord["type"], Record<string, unknown>> = {
  "task.created": { title: "Test task", kind: "task", priority: 1, status: "open" },
  "task.updated": {},
  "task.status_set": { status: "in_progress" },
  "task.claimed": {},
  "task.noted": { text: "A note" },
  "task.spec_attached": {
    spec_path: ".tasque/specs/tsq-abc123/spec.md",
    spec_fingerprint: "abc123",
    spec_attached_at: "2026-02-17T00:00:00.000Z",
    spec_attached_by: "test",
  },
  "task.superseded": { with: "tsq-other1" },
  "dep.added": { blocker: "tsq-other1" },
  "dep.removed": { blocker: "tsq-other1" },
  "link.added": { target: "tsq-other1", type: "relates_to" },
  "link.removed": { target: "tsq-other1", type: "relates_to" },
};

function event(id: string, type: EventRecord["type"]): EventRecord {
  return {
    id,
    event_id: id,
    ts: "2026-02-17T00:00:00.000Z",
    actor: "test",
    type,
    task_id: "tsq-abc123",
    payload: { ...MINIMAL_PAYLOADS[type] },
  };
}

describe("store events", () => {
  it("appends and reads events", async () => {
    const repo = await makeRepo();
    const records = [event("01AAAAAA", "task.created"), event("01BBBBBB", "task.updated")];

    await appendEvents(repo, records);
    const result = await readEvents(repo);

    expect(result.warning).toBeUndefined();
    expect(result.events).toEqual(records);
  });

  it("rejects event missing event_id with EVENTS_CORRUPT error", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });
    const invalid = JSON.stringify({
      ts: "2026-02-17T00:00:00.000Z",
      actor: "test",
      type: "task.created",
      task_id: "tsq-abc123",
      payload: {},
    });
    await writeFile(paths.eventsFile, `${invalid}\n`, "utf8");

    const err = await readEvents(repo).catch((e) => e);
    expect(err).toBeInstanceOf(TsqError);
    expect(err.code).toBe("EVENTS_CORRUPT");
    expect(err.exitCode).toBe(2);
  });

  it("rejects event with non-string ts at correct line number", async () => {
    const repo = await makeRepo();
    const valid = event("01AAAAAA", "task.created");
    await appendEvents(repo, [valid]);
    const paths = getPaths(repo);
    const invalid = JSON.stringify({
      event_id: "01BBBBBB",
      ts: 12345,
      actor: "test",
      type: "task.updated",
      task_id: "tsq-abc123",
      payload: {},
    });
    await appendFile(paths.eventsFile, `${invalid}\n`, "utf8");

    const err = await readEvents(repo).catch((e) => e);
    expect(err).toBeInstanceOf(TsqError);
    expect(err.code).toBe("EVENTS_CORRUPT");
    expect(err.exitCode).toBe(2);
    expect(err.message).toContain("line 2");
  });

  it("rejects event missing task_id field", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });
    const invalid = JSON.stringify({
      event_id: "01AAAAAA",
      ts: "2026-02-17T00:00:00.000Z",
      actor: "test",
      type: "task.created",
      payload: {},
    });
    await writeFile(paths.eventsFile, `${invalid}\n`, "utf8");

    const err = await readEvents(repo).catch((e) => e);
    expect(err).toBeInstanceOf(TsqError);
    expect(err.code).toBe("EVENTS_CORRUPT");
    expect(err.exitCode).toBe(2);
  });

  it("accepts valid events without schema validation errors", async () => {
    const repo = await makeRepo();
    const records = [event("01AAAAAA", "task.created"), event("01BBBBBB", "task.updated")];
    await appendEvents(repo, records);

    const result = await readEvents(repo);
    expect(result.events).toEqual(records);
    expect(result.warning).toBeUndefined();
  });

  it("ignores malformed trailing line and returns warning", async () => {
    const repo = await makeRepo();
    const record = event("01CCCCCC", "task.created");

    await appendEvents(repo, [record]);
    await appendFile(getPaths(repo).eventsFile, '{"event_id":', "utf8");

    const result = await readEvents(repo);

    expect(result.events).toEqual([record]);
    expect(result.warning).toBeString();
  });

  it("throws EVENTS_CORRUPT for corrupt middle line that is not valid JSON", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });

    const validFirst = JSON.stringify(event("01AAAAAA", "task.created"));
    const corruptMiddle = "NOT-VALID-JSON{{{";
    const validLast = JSON.stringify(event("01BBBBBB", "task.updated"));
    await writeFile(paths.eventsFile, `${validFirst}\n${corruptMiddle}\n${validLast}\n`, "utf8");

    const err = await readEvents(repo).catch((e) => e);
    expect(err).toBeInstanceOf(TsqError);
    expect(err.code).toBe("EVENTS_CORRUPT");
    expect(err.exitCode).toBe(2);
    expect(err.message).toContain("line 2");
  });

  it("returns empty array for empty events.jsonl file", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });
    await writeFile(paths.eventsFile, "", "utf8");

    const result = await readEvents(repo);

    expect(result.events).toEqual([]);
    expect(result.warning).toBeUndefined();
  });

  it("throws EVENTS_CORRUPT for single line with valid JSON but missing required fields", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });
    await writeFile(paths.eventsFile, `${JSON.stringify({ foo: "bar" })}\n`, "utf8");

    const err = await readEvents(repo).catch((e) => e);
    expect(err).toBeInstanceOf(TsqError);
    expect(err.code).toBe("EVENTS_CORRUPT");
    expect(err.exitCode).toBe(2);
  });

  it("rejects dep events with invalid dep_type value", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });
    const invalid = JSON.stringify({
      event_id: "01BADDEP",
      ts: "2026-02-17T00:00:00.000Z",
      actor: "test",
      type: "dep.added",
      task_id: "tsq-abc123",
      payload: { blocker: "tsq-other1", dep_type: "invalid_type" },
    });
    await writeFile(paths.eventsFile, `${invalid}\n`, "utf8");

    const err = await readEvents(repo).catch((e) => e);
    expect(err).toBeInstanceOf(TsqError);
    expect(err.code).toBe("EVENTS_CORRUPT");
    expect(err.exitCode).toBe(2);
  });

  it("throws EVENTS_CORRUPT for corrupt middle line that is valid JSON but missing required fields", async () => {
    const repo = await makeRepo();
    const paths = getPaths(repo);
    await mkdir(paths.tasqueDir, { recursive: true });

    const validFirst = JSON.stringify(event("01AAAAAA", "task.created"));
    const invalidMiddle = JSON.stringify({ random: "object", no_event_id: true });
    const validLast = JSON.stringify(event("01BBBBBB", "task.updated"));
    await writeFile(paths.eventsFile, `${validFirst}\n${invalidMiddle}\n${validLast}\n`, "utf8");

    const err = await readEvents(repo).catch((e) => e);
    expect(err).toBeInstanceOf(TsqError);
    expect(err.code).toBe("EVENTS_CORRUPT");
    expect(err.exitCode).toBe(2);
    expect(err.message).toContain("line 2");
  });
});
