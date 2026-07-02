import { describe, expect, it } from "bun:test";
import {
	parseDependencyEnvelope,
	parseTasksEnvelope,
	runTsq,
} from "../src/data";
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

	it("warns on a null payload", () => {
		const result = parseTasksEnvelope("null");
		expect(result.tasks).toEqual([]);
		expect(result.warning).toBe("Unexpected payload from tsq watch --once");
	});

	it("warns on a non-object payload", () => {
		const result = parseTasksEnvelope(JSON.stringify("oops"));
		expect(result.tasks).toEqual([]);
		expect(result.warning).toBe("Unexpected payload from tsq watch --once");
	});

	it("warns on a non-boolean ok field", () => {
		const result = parseTasksEnvelope(
			JSON.stringify({ ok: "false", data: { tasks: [sampleTask] } }),
		);
		expect(result.tasks).toEqual([]);
		expect(result.warning).toBe("Unexpected payload from tsq watch --once");
	});

	it("warns on a non-array tasks field", () => {
		const result = parseTasksEnvelope(
			JSON.stringify({ ok: true, data: { tasks: "not-a-list" } }),
		);
		expect(result.tasks).toEqual([]);
		expect(result.warning).toBe("Task payload missing tasks array");
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

	it("warns on a null payload", () => {
		const result = parseDependencyEnvelope("null");
		expect(result.root).toBeUndefined();
		expect(result.warning).toBe("Unexpected payload from tsq deps");
	});

	it("warns on a non-object payload", () => {
		const result = parseDependencyEnvelope(JSON.stringify(42));
		expect(result.root).toBeUndefined();
		expect(result.warning).toBe("Unexpected payload from tsq deps");
	});

	it("warns on a non-boolean ok field", () => {
		const result = parseDependencyEnvelope(JSON.stringify({ ok: "true" }));
		expect(result.root).toBeUndefined();
		expect(result.warning).toBe("Unexpected payload from tsq deps");
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

describe("runTsq", () => {
	it("returns stdout, stderr, and exitCode for a normal process", async () => {
		const result = await runTsq([
			process.execPath,
			"-e",
			"process.stdout.write('hi'); process.stderr.write('bye'); process.exit(3)",
		]);

		expect(result).toEqual({ exitCode: 3, stdout: "hi", stderr: "bye" });
	});

	it("returns a non-zero exit when a process times out", async () => {
		const start = Date.now();
		const result = await runTsq(
			[process.execPath, "-e", "setInterval(() => {}, 1000)"],
			{ timeoutMs: 300 },
		);

		expect(result.exitCode).not.toBe(0);
		expect(Date.now() - start).toBeLessThan(5000);
	});
});
