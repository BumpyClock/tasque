import { afterEach, describe, expect, it } from "bun:test";
import { access, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  buildReleaseNotes,
  flattenTree,
  generateReleaseNotesArtifacts,
  renderReleaseNotesJson,
  renderReleaseNotesMarkdown,
  selectReleaseTasks,
} from "../scripts/release-hooks";

interface TaskTreeNode {
  task: {
    id: string;
    title: string;
    kind: "task" | "feature" | "epic";
    status: string;
    priority: number;
    assignee?: string;
    parent_id?: string;
    closed_at?: string;
  };
  children: TaskTreeNode[];
}

const repos: string[] = [];

async function makeTempDir(prefix: string): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), prefix));
  repos.push(dir);
  return dir;
}

afterEach(async () => {
  await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
});

describe("release hooks", () => {
  it("flattens tree nodes and selects only closed tasks after baseline", () => {
    const tree: TaskTreeNode[] = [
      {
        task: {
          id: "tsq-root1",
          title: "Root feature",
          kind: "feature",
          status: "closed",
          priority: 2,
          closed_at: "2026-01-03T00:00:00.000Z",
        },
        children: [
          {
            task: {
              id: "tsq-root1.1",
              title: "Child task",
              kind: "task",
              status: "closed",
              priority: 0,
              parent_id: "tsq-root1",
              closed_at: "2026-01-05T00:00:00.000Z",
            },
            children: [],
          },
          {
            task: {
              id: "tsq-root1.2",
              title: "Still open",
              kind: "task",
              status: "open",
              priority: 1,
            },
            children: [],
          },
        ],
      },
      {
        task: {
          id: "tsq-root2",
          title: "Second root epic",
          kind: "epic",
          status: "closed",
          priority: 1,
          closed_at: "2026-01-06T00:00:00.000Z",
        },
        children: [],
      },
    ];

    const flattened = flattenTree(tree);
    expect(flattened.map((task) => task.id)).toEqual([
      "tsq-root1",
      "tsq-root1.1",
      "tsq-root1.2",
      "tsq-root2",
    ]);

    const selected = selectReleaseTasks(flattened, "2026-01-04T00:00:00.000Z");
    expect(selected.map((task) => task.id)).toEqual(["tsq-root2", "tsq-root1.1"]);
    expect(selected[1]?.parent_id).toBe("tsq-root1");
  });

  it("excludes tasks at or before baseline and tasks with invalid closed_at", () => {
    const tasks = [
      {
        id: "tsq-before",
        title: "Before baseline",
        kind: "task" as const,
        status: "closed",
        priority: 3,
        closed_at: "2026-02-01T09:59:59.000Z",
      },
      {
        id: "tsq-at",
        title: "At baseline",
        kind: "task" as const,
        status: "closed",
        priority: 3,
        closed_at: "2026-02-01T10:00:00.000Z",
      },
      {
        id: "tsq-after",
        title: "After baseline",
        kind: "task" as const,
        status: "closed",
        priority: 1,
        closed_at: "2026-02-01T10:00:01.000Z",
      },
      {
        id: "tsq-invalid",
        title: "Invalid close timestamp",
        kind: "feature" as const,
        status: "closed",
        priority: 2,
        closed_at: "not-a-date",
      },
      {
        id: "tsq-missing",
        title: "Missing close timestamp",
        kind: "epic" as const,
        status: "closed",
        priority: 0,
      },
      {
        id: "tsq-open",
        title: "Open task",
        kind: "task" as const,
        status: "open",
        priority: 2,
        closed_at: "2026-02-02T00:00:00.000Z",
      },
    ];

    const selected = selectReleaseTasks(tasks, "2026-02-01T10:00:00.000Z");
    expect(selected.map((task) => task.id)).toEqual(["tsq-after"]);
  });

  it("renders markdown and json release notes with expected sections and fields", () => {
    const notes = buildReleaseNotes(
      "1.2.3",
      { tag: "v1.2.2", ts: "2026-02-10T00:00:00.000Z" },
      [
        {
          id: "tsq-epic",
          title: "Epic work",
          kind: "epic",
          priority: 0,
          closed_at: "2026-02-12T00:00:00.000Z",
        },
        {
          id: "tsq-feature",
          title: "Feature work",
          kind: "feature",
          priority: 1,
          closed_at: "2026-02-11T00:00:00.000Z",
          assignee: "ada",
        },
        {
          id: "tsq-task",
          title: "Task work",
          kind: "task",
          priority: 2,
          closed_at: "2026-02-10T12:00:00.000Z",
        },
      ],
      "2026-02-13T00:00:00.000Z",
    );

    const markdown = renderReleaseNotesMarkdown(notes);
    expect(markdown).toContain("# Release Notes v1.2.3");
    expect(markdown).toContain("generated_at: 2026-02-13T00:00:00.000Z");
    expect(markdown).toContain("baseline: v1.2.2 (2026-02-10T00:00:00.000Z)");
    expect(markdown).toContain("## Summary");
    expect(markdown).toContain("- total: 3");
    expect(markdown).toContain("## Epics");
    expect(markdown).toContain("## Features");
    expect(markdown).toContain("## Tasks");
    expect(markdown).toContain(
      "[tsq-feature] Feature work (p1, closed 2026-02-11T00:00:00.000Z @ada)",
    );

    const json = renderReleaseNotesJson(notes);
    const parsed = JSON.parse(json) as Record<string, unknown>;
    expect(parsed.schema_version).toBe(1);
    expect(parsed.version).toBe("1.2.3");
    expect(parsed.generated_at).toBe("2026-02-13T00:00:00.000Z");
    expect(parsed.baseline).toEqual({ tag: "v1.2.2", ts: "2026-02-10T00:00:00.000Z" });
    expect(parsed.counts).toEqual({
      total: 3,
      task: 1,
      feature: 1,
      epic: 1,
      p0: 1,
      p1: 1,
      p2: 1,
      p3: 0,
    });
    expect(parsed.items).toBeArray();
  });

  it("generates release notes artifacts using provided command runner", async () => {
    const repoRoot = await makeTempDir("tasque-release-hooks-repo-");
    const releaseDir = join(repoRoot, "dist", "releases");
    const calls: Array<{ cmd: string[]; cwd: string }> = [];

    const runCommand = async (cmd: string[], cwd: string) => {
      calls.push({ cmd, cwd });
      if (cmd[0] === "git") {
        return {
          code: 0,
          stdout: "v1.0.0\t2026-02-01T00:00:00.000Z\n",
          stderr: "",
        };
      }
      if (cmd[0] === "tsq") {
        return {
          code: 0,
          stdout: JSON.stringify({
            ok: true,
            data: {
              tree: [
                {
                  task: {
                    id: "tsq-closed-new",
                    title: "Closed after baseline",
                    kind: "task",
                    status: "closed",
                    priority: 1,
                    closed_at: "2026-02-03T00:00:00.000Z",
                  },
                  children: [],
                },
                {
                  task: {
                    id: "tsq-closed-old",
                    title: "Closed before baseline",
                    kind: "task",
                    status: "closed",
                    priority: 2,
                    closed_at: "2026-01-31T00:00:00.000Z",
                  },
                  children: [],
                },
              ],
            },
          }),
          stderr: "",
        };
      }
      return { code: 1, stdout: "", stderr: `unexpected command: ${cmd.join(" ")}` };
    };

    const result = await generateReleaseNotesArtifacts({
      repoRoot,
      releaseDir,
      version: "1.0.1",
      tsqBin: "tsq",
      generatedAt: "2026-02-04T00:00:00.000Z",
      runCommand,
    });

    expect(calls.length).toBe(2);
    expect(calls[0]?.cmd[0]).toBe("git");
    expect(calls[1]?.cmd).toEqual(["tsq", "list", "--tree", "--full", "--json"]);
    expect(calls.every((call) => call.cwd === repoRoot)).toBe(true);

    expect(result.baseline).toEqual({ tag: "v1.0.0", ts: "2026-02-01T00:00:00.000Z" });
    expect(result.notes.version).toBe("1.0.1");
    expect(result.notes.items.map((task) => task.id)).toEqual(["tsq-closed-new"]);

    await access(result.markdownPath);
    await access(result.jsonPath);

    const markdown = await readFile(result.markdownPath, "utf8");
    expect(markdown).toContain("# Release Notes v1.0.1");
    expect(markdown).toContain("baseline: v1.0.0 (2026-02-01T00:00:00.000Z)");

    const json = JSON.parse(await readFile(result.jsonPath, "utf8")) as {
      version: string;
      baseline: { tag: string; ts: string } | null;
      items: Array<{ id: string }>;
    };
    expect(json.version).toBe("1.0.1");
    expect(json.baseline).toEqual({ tag: "v1.0.0", ts: "2026-02-01T00:00:00.000Z" });
    expect(json.items.map((item) => item.id)).toEqual(["tsq-closed-new"]);
  });
});
