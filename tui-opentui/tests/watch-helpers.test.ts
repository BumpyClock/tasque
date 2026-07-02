import { describe, expect, test } from "bun:test";
import type { TasqueTask } from "../src/model";
import {
	buildWatchRows,
	filterLabel,
	metaBadge,
} from "../src/watch-helpers";

function task(overrides: Partial<TasqueTask> & { id: string }): TasqueTask {
	return {
		kind: "task",
		title: `Title ${overrides.id}`,
		status: "open",
		priority: 2,
		labels: [],
		created_at: "2026-01-01T00:00:00Z",
		updated_at: "2026-01-01T00:00:00Z",
		...overrides,
	};
}

describe("buildWatchRows", () => {
	test("flat mode returns sorted rows with empty prefixes", () => {
		const rows = buildWatchRows(
			[
				task({ id: "tsq-2", status: "open" }),
				task({ id: "tsq-1", status: "in_progress" }),
			],
			false,
		);
		// in_progress sorts before open.
		expect(rows.map((row) => row.task.id)).toEqual(["tsq-1", "tsq-2"]);
		expect(rows.every((row) => row.prefix === "")).toBe(true);
	});

	test("tree mode indents children under their parent", () => {
		const rows = buildWatchRows(
			[
				task({ id: "tsq-1", kind: "epic" }),
				task({ id: "tsq-2", parent_id: "tsq-1" }),
			],
			true,
		);
		const child = rows.find((row) => row.task.id === "tsq-2");
		expect(child?.prefix.length ?? 0).toBeGreaterThan(0);
		const parent = rows.find((row) => row.task.id === "tsq-1");
		expect(parent?.prefix).toBe("");
	});
});

describe("filterLabel", () => {
	test("status only", () => {
		expect(filterLabel("open,in_progress")).toBe("status:open,in_progress");
	});
	test("with assignee", () => {
		expect(filterLabel("open", "alice")).toBe("status:open assignee:alice");
	});
});

describe("metaBadge", () => {
	test("priority only", () => {
		expect(metaBadge(task({ id: "tsq-1", priority: 3 }))).toBe("p3");
	});
	test("priority and assignee", () => {
		expect(metaBadge(task({ id: "tsq-1", priority: 1, assignee: "bob" }))).toBe(
			"p1 @bob",
		);
	});
});
