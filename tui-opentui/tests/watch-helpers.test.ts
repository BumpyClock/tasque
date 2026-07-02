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
	test("flat mode sorts by the full status priority order", () => {
		// Shuffled input across every status; expect the watch status order.
		const rows = buildWatchRows(
			[
				task({ id: "c", status: "canceled" }),
				task({ id: "o", status: "open" }),
				task({ id: "x", status: "closed" }),
				task({ id: "i", status: "in_progress" }),
				task({ id: "d", status: "deferred" }),
				task({ id: "b", status: "blocked" }),
			],
			false,
		);
		expect(rows.map((row) => row.task.id)).toEqual([
			"i",
			"o",
			"b",
			"d",
			"x",
			"c",
		]);
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

	test("tree mode nests grandchildren deeper than children", () => {
		const rows = buildWatchRows(
			[
				task({ id: "root", kind: "epic" }),
				task({ id: "child", parent_id: "root" }),
				task({ id: "grandchild", parent_id: "child" }),
			],
			true,
		);
		const prefixLen = (id: string) =>
			rows.find((row) => row.task.id === id)?.prefix.length ?? 0;
		expect(prefixLen("root")).toBe(0);
		expect(prefixLen("grandchild")).toBeGreaterThan(prefixLen("child"));
	});

	test("tree mode treats a task with an unknown parent as a root", () => {
		const rows = buildWatchRows(
			[task({ id: "orphan", parent_id: "does-not-exist" })],
			true,
		);
		expect(rows).toHaveLength(1);
		expect(rows[0]?.prefix).toBe("");
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
