import { describe, expect, it } from "bun:test";
import { parseDependencyEnvelope, parseTasksEnvelope } from "../src/data";
import { parseSpecEnvelope } from "../src/tui-helpers";

const sampleTask = {
  id: "tsq-1",
  kind: "task",
  title: "demo",
  status: "open",
  priority: 2,
  labels: [],
  created_at: "2026-01-01T00:00:00.000Z",
  updated_at: "2026-01-01T00:00:00.000Z",
};

describe("parseTasksEnvelope", () => {
  it("returns tasks from a valid watch envelope", () => {
    const stdout = JSON.stringify({
      ok: true,
      data: { frame_ts: "2026-01-01T00:00:00.000Z", tasks: [sampleTask] },
    });
    const result = parseTasksEnvelope(stdout);
    expect(result.warning).toBeUndefined();
    expect(result.tasks).toHaveLength(1);
    expect(result.tasks[0]?.id).toBe("tsq-1");
  });

  it("surfaces the error message from an ok:false envelope", () => {
    const stdout = JSON.stringify({
      ok: false,
      error: { code: "VALIDATION_ERROR", message: "bad filter" },
    });
    const result = parseTasksEnvelope(stdout);
    expect(result.tasks).toEqual([]);
    expect(result.warning).toBe("bad filter");
  });

  it("warns on malformed JSON", () => {
    const result = parseTasksEnvelope("not json");
    expect(result.tasks).toEqual([]);
    expect(result.warning).toBe(
      "Unable to parse JSON output from tsq watch --once",
    );
  });

  it("returns an empty task list when data is missing", () => {
    const result = parseTasksEnvelope(JSON.stringify({ ok: true }));
    expect(result.tasks).toEqual([]);
    expect(result.warning).toBeUndefined();
  });
});

describe("parseDependencyEnvelope", () => {
  it("returns the root node from a valid deps envelope", () => {
    const stdout = JSON.stringify({
      ok: true,
      data: {
        root: {
          id: "tsq-1",
          task: sampleTask,
          children: [
            {
              id: "tsq-2",
              dep_type: "blocks",
              direction: "outgoing",
              children: [],
            },
          ],
        },
      },
    });
    const result = parseDependencyEnvelope(stdout);
    expect(result.warning).toBeUndefined();
    expect(result.root?.id).toBe("tsq-1");
    expect(result.root?.task?.title).toBe("demo");
    expect(result.root?.children).toHaveLength(1);
    expect(result.root?.children[0]?.depType).toBe("blocks");
  });

  it("surfaces the error message from an ok:false envelope", () => {
    const stdout = JSON.stringify({
      ok: false,
      error: { message: "task not found" },
    });
    expect(parseDependencyEnvelope(stdout).warning).toBe("task not found");
  });

  it("warns on malformed JSON", () => {
    expect(parseDependencyEnvelope("{oops").warning).toBe(
      "Unable to parse JSON output from tsq deps",
    );
  });

  it("warns when the root node is missing", () => {
    const result = parseDependencyEnvelope(
      JSON.stringify({ ok: true, data: {} }),
    );
    expect(result.root).toBeUndefined();
    expect(result.warning).toBe("Dependency tree payload missing root node");
  });
});

describe("parseSpecEnvelope", () => {
  it("splits data.spec.content into lines", () => {
    const stdout = JSON.stringify({
      ok: true,
      data: {
        spec: {
          path: ".tasque/specs/tsq-1/spec.md",
          fingerprint: "abc123",
          content: "# hello\r\nworld",
        },
      },
    });
    const result = parseSpecEnvelope(stdout);
    expect(result.warning).toBeUndefined();
    expect(result.lines).toEqual(["# hello", "world"]);
  });

  it("returns a placeholder for empty content", () => {
    const stdout = JSON.stringify({
      ok: true,
      data: { spec: { path: "p", fingerprint: "f", content: "" } },
    });
    expect(parseSpecEnvelope(stdout).lines).toEqual(["(empty spec)"]);
  });

  it("surfaces the error message from an ok:false envelope", () => {
    const stdout = JSON.stringify({
      ok: false,
      error: { message: "no spec attached" },
    });
    const result = parseSpecEnvelope(stdout);
    expect(result.lines).toEqual([]);
    expect(result.warning).toBe("no spec attached");
  });

  it("warns on malformed JSON", () => {
    expect(parseSpecEnvelope("").warning).toBe(
      "Unable to parse JSON output from tsq spec --show",
    );
  });

  it("warns when data.spec.content is missing", () => {
    const result = parseSpecEnvelope(
      JSON.stringify({ ok: true, data: { spec: { path: "p" } } }),
    );
    expect(result.lines).toEqual([]);
    expect(result.warning).toBe("Spec payload missing content");
  });
});
