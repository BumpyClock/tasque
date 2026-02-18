import { afterEach, describe, expect, it } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { TasqueService } from "../src/app/service";

const repos: string[] = [];

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-app-stale-"));
  repos.push(repo);
  return repo;
}

function sequenceNow(values: string[]): () => string {
  const queue = [...values];
  return () => {
    const next = queue.shift();
    if (!next) {
      throw new Error("now() sequence exhausted");
    }
    return next;
  };
}

afterEach(async () => {
  await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
});

describe("service stale", () => {
  it("returns stale tasks ordered by updated_at, then priority, then id", async () => {
    const repo = await makeRepo();
    const service = new TasqueService(
      repo,
      "test-stale",
      sequenceNow([
        "2026-01-01T00:00:00.000Z",
        "2026-01-02T00:00:00.000Z",
        "2026-01-02T00:00:00.000Z",
        "2026-01-02T00:00:00.000Z",
        "2026-02-10T00:00:00.000Z",
      ]),
    );

    await service.init();
    const oldest = await service.create({ title: "oldest", kind: "task", priority: 2 });
    const sameTsHigherPriority = await service.create({
      title: "same-ts-higher-priority",
      kind: "task",
      priority: 2,
    });
    const sameTsLowerPriorityA = await service.create({
      title: "same-ts-lower-priority-a",
      kind: "task",
      priority: 0,
    });
    const sameTsLowerPriorityB = await service.create({
      title: "same-ts-lower-priority-b",
      kind: "task",
      priority: 0,
    });

    const result = await service.stale({ days: 30 });
    const lowPriorityIds = [sameTsLowerPriorityA.id, sameTsLowerPriorityB.id].sort((a, b) =>
      a.localeCompare(b),
    );

    expect(result.days).toBe(30);
    expect(result.cutoff).toBe("2026-01-11T00:00:00.000Z");
    expect(result.statuses).toEqual(["open", "in_progress", "blocked", "deferred"]);
    expect(result.tasks.map((task) => task.id)).toEqual([
      oldest.id,
      ...lowPriorityIds,
      sameTsHigherPriority.id,
    ]);
  });

  it("rejects invalid days values", async () => {
    const repo = await makeRepo();
    const service = new TasqueService(repo, "test-stale", () => "2026-02-10T00:00:00.000Z");
    await service.init();

    await expect(service.stale({ days: -1 })).rejects.toMatchObject({
      code: "VALIDATION_ERROR",
    });
    await expect(service.stale({ days: 1.25 })).rejects.toMatchObject({
      code: "VALIDATION_ERROR",
    });
  });
});
