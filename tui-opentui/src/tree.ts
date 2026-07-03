import type { TasqueTask } from "./model";
import { clampIndex } from "./tui-helpers";
import type { TreeLine } from "./tui-types";

export function buildTreePrefix(line: TreeLine): string {
	if (line.depth <= 0) {
		return "";
	}
	// The last trail entry describes this node itself (its own ├─/└─ connector
	// covers it); only the entries above it are ancestor guide columns.
	const ancestors = buildAncestorPrefix(line.siblingTrail.slice(0, -1));
	const own = line.isLastSibling ? "└─" : "├─";
	return `${ancestors}${own} `;
}

function buildAncestorPrefix(siblingTrail: boolean[]): string {
	return siblingTrail
		.map((hasMoreSiblings) => (hasMoreSiblings ? "│ " : "  "))
		.join("");
}

export function buildTreeLines(
	tasks: TasqueTask[],
	collapsed?: ReadonlySet<string>,
): TreeLine[] {
	const byParent = new Map<string, TasqueTask[]>();
	const byId = new Map(tasks.map((task) => [task.id, task]));

	for (const task of tasks) {
		const parent = task.parent_id;
		if (!parent || !byId.has(parent)) {
			continue;
		}
		const list = byParent.get(parent) ?? [];
		list.push(task);
		byParent.set(parent, list);
	}

	const roots = tasks.filter(
		(task) => !task.parent_id || !byId.has(task.parent_id),
	);
	const output: TreeLine[] = [];

	// Memoized so the whole tree is counted once (O(n)) instead of once per
	// ancestor, which would go quadratic on deep task lists.
	const descendantCountById = new Map<string, number>();
	const countDescendants = (task: TasqueTask): number => {
		const cached = descendantCountById.get(task.id);
		if (cached !== undefined) {
			return cached;
		}
		const children = byParent.get(task.id) ?? [];
		const count = children.reduce(
			(sum, child) => sum + 1 + countDescendants(child),
			0,
		);
		descendantCountById.set(task.id, count);
		return count;
	};

	const walk = (
		task: TasqueTask,
		depth: number,
		siblingTrail: boolean[],
		isLastSibling: boolean,
	) => {
		const children = byParent.get(task.id) ?? [];
		const hasChildren = children.length > 0;
		const isCollapsed = hasChildren && (collapsed?.has(task.id) ?? false);
		output.push({
			task,
			depth,
			isLastSibling,
			siblingTrail,
			hasChildren,
			isCollapsed,
			descendantCount: hasChildren ? countDescendants(task) : 0,
		});
		if (isCollapsed) {
			return;
		}
		children.forEach((child, index) => {
			walk(
				child,
				depth + 1,
				[...siblingTrail, index < children.length - 1],
				index === children.length - 1,
			);
		});
	};

	roots.forEach((root, rootIndex) => {
		walk(root, 0, [], rootIndex >= roots.length - 1);
	});

	return output;
}

// Marker rendered between the tree guides and the title: ▾ expanded parent,
// ▸ collapsed parent, blank for leaves so sibling titles stay aligned.
export function treeMarker(hasChildren: boolean, isCollapsed: boolean): string {
	if (!hasChildren) {
		return " ";
	}
	return isCollapsed ? "▸" : "▾";
}

// IDs that can be folded: parents whose children are present in the list.
export function collapsibleTaskIds(tasks: TasqueTask[]): Set<string> {
	const byId = new Set(tasks.map((task) => task.id));
	const parents = new Set<string>();
	for (const task of tasks) {
		if (task.parent_id && byId.has(task.parent_id)) {
			parents.add(task.parent_id);
		}
	}
	return parents;
}

// Width of the ID column sized to the longest visible ID, so nested IDs like
// `tsq-98.3.12` are never truncated. Clamped to keep pathological IDs from
// eating the whole row.
export function idColumnWidth(ids: string[]): number {
	let width = 6;
	for (const id of ids) {
		if (id.length > width) {
			width = id.length;
		}
	}
	return Math.min(width, 24);
}

// Keep selection stable across fold/unfold: prefer the same task, else its
// nearest visible ancestor, else clamp the previous index.
export function resolveSelectionIndex(
	visibleIds: string[],
	targetId: string | undefined,
	tasks: TasqueTask[],
	fallbackIndex: number,
): number {
	if (targetId) {
		const indexById = new Map(visibleIds.map((id, index) => [id, index]));
		const byId = new Map(tasks.map((task) => [task.id, task]));
		const seen = new Set<string>();
		let current: string | undefined = targetId;
		while (current && !seen.has(current)) {
			const index = indexById.get(current);
			if (index !== undefined) {
				return index;
			}
			seen.add(current);
			current = byId.get(current)?.parent_id;
		}
	}
	return clampIndex(fallbackIndex, visibleIds.length);
}
