import { describe, expect, test } from "bun:test";
import type { TasqueTask } from "../src/model";
import {
	buildTreeLines,
	buildTreePrefix,
	collapsibleTaskIds,
	idColumnWidth,
	resolveSelectionIndex,
	treeMarker,
} from "../src/tree";

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

const FAMILY = [
	task({ id: "tsq-1", kind: "epic" }),
	task({ id: "tsq-1.1", parent_id: "tsq-1" }),
	task({ id: "tsq-1.1.1", parent_id: "tsq-1.1" }),
	task({ id: "tsq-1.2", parent_id: "tsq-1" }),
	task({ id: "tsq-2" }),
];

describe("buildTreeLines fold state", () => {
	test("marks parents with child and descendant counts", () => {
		const lines = buildTreeLines(FAMILY);
		const byId = new Map(lines.map((line) => [line.task.id, line]));
		expect(byId.get("tsq-1")?.hasChildren).toBe(true);
		expect(byId.get("tsq-1")?.descendantCount).toBe(3);
		expect(byId.get("tsq-1.1")?.descendantCount).toBe(1);
		expect(byId.get("tsq-1.1.1")?.hasChildren).toBe(false);
		expect(byId.get("tsq-2")?.hasChildren).toBe(false);
	});

	test("collapsing a parent hides its whole subtree", () => {
		const lines = buildTreeLines(FAMILY, new Set(["tsq-1"]));
		expect(lines.map((line) => line.task.id)).toEqual(["tsq-1", "tsq-2"]);
		expect(lines[0]?.isCollapsed).toBe(true);
		expect(lines[0]?.descendantCount).toBe(3);
	});

	test("collapsing a mid-level parent keeps its siblings visible", () => {
		const lines = buildTreeLines(FAMILY, new Set(["tsq-1.1"]));
		expect(lines.map((line) => line.task.id)).toEqual([
			"tsq-1",
			"tsq-1.1",
			"tsq-1.2",
			"tsq-2",
		]);
	});

	test("collapse ids for leaves are ignored", () => {
		const lines = buildTreeLines(FAMILY, new Set(["tsq-2"]));
		expect(lines).toHaveLength(FAMILY.length);
		expect(lines.every((line) => !line.isCollapsed)).toBe(true);
	});
});

describe("buildTreePrefix", () => {
	test("draws exact guide glyphs without phantom ancestor bars", () => {
		const lines = buildTreeLines(FAMILY);
		const prefixById = new Map(
			lines.map((line) => [line.task.id, buildTreePrefix(line)]),
		);
		expect(prefixById.get("tsq-1")).toBe("");
		expect(prefixById.get("tsq-1.1")).toBe("├─ ");
		expect(prefixById.get("tsq-1.2")).toBe("└─ ");
		// tsq-1.1 has a following sibling (tsq-1.2), so its child carries one
		// `│ ` guide column plus its own last-child connector.
		expect(prefixById.get("tsq-1.1.1")).toBe("│ └─ ");
		expect(prefixById.get("tsq-2")).toBe("");
	});
});

describe("treeMarker", () => {
	test("parent expanded / collapsed / leaf", () => {
		expect(treeMarker(true, false)).toBe("▾");
		expect(treeMarker(true, true)).toBe("▸");
		expect(treeMarker(false, false)).toBe(" ");
	});
});

describe("collapsibleTaskIds", () => {
	test("returns only parents whose children are in the list", () => {
		const ids = collapsibleTaskIds(FAMILY);
		expect([...ids].sort()).toEqual(["tsq-1", "tsq-1.1"]);
	});

	test("ignores parents that are not present themselves", () => {
		const ids = collapsibleTaskIds([task({ id: "a", parent_id: "ghost" })]);
		expect(ids.size).toBe(0);
	});
});

describe("idColumnWidth", () => {
	test("fits the longest id", () => {
		expect(idColumnWidth(["tsq-1", "tsq-98.3.12"])).toBe(11);
	});
	test("keeps a floor for short ids and a ceiling for huge ones", () => {
		expect(idColumnWidth(["a"])).toBe(6);
		expect(idColumnWidth(["x".repeat(60)])).toBe(24);
	});
});

describe("resolveSelectionIndex", () => {
	const visible = ["tsq-1", "tsq-2"];

	test("keeps the same task when still visible", () => {
		expect(resolveSelectionIndex(visible, "tsq-2", FAMILY, 0)).toBe(1);
	});

	test("falls back to the nearest visible ancestor", () => {
		expect(resolveSelectionIndex(visible, "tsq-1.1.1", FAMILY, 4)).toBe(0);
	});

	test("clamps the previous index when nothing matches", () => {
		expect(resolveSelectionIndex(visible, "unknown", FAMILY, 9)).toBe(1);
		expect(resolveSelectionIndex([], "unknown", FAMILY, 3)).toBe(0);
	});
});
