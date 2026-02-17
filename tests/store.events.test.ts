import { afterEach, describe, expect, it } from "bun:test";
import { appendFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

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

function event(id: string, type: EventRecord["type"]): EventRecord {
  return {
    event_id: id,
    ts: "2026-02-17T00:00:00.000Z",
    actor: "test",
    type,
    task_id: "tsq-abc123",
    payload: {},
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

  it("ignores malformed trailing line and returns warning", async () => {
    const repo = await makeRepo();
    const record = event("01CCCCCC", "task.created");

    await appendEvents(repo, [record]);
    await appendFile(getPaths(repo).eventsFile, '{"event_id":', "utf8");

    const result = await readEvents(repo);

    expect(result.events).toEqual([record]);
    expect(result.warning).toBeString();
  });
});
