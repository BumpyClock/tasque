import { TsqError } from "../errors";
import type { State, Task } from "../types";

/** Direction to walk the dependency graph from the root task. */
export type DepDirection = "up" | "down" | "both";

/**
 * A node in a dependency tree.
 *
 * - `id`: task ID
 * - `task`: the Task record from state
 * - `direction`: the direction this node represents relative to the walk
 * - `depth`: 0 for root, increments per level
 * - `children`: sub-nodes in the same walk direction
 */
export interface DepTreeNode {
  id: string;
  task: Task;
  direction: DepDirection;
  depth: number;
  children: DepTreeNode[];
}

const DEFAULT_MAX_DEPTH = 10;

/**
 * Build a dependency tree starting from rootId.
 *
 * Direction semantics:
 * - "up"   = walk the blockers chain (what blocks rootId, and what blocks those, etc.)
 * - "down" = walk the dependents chain (what rootId is blocking, transitively)
 * - "both" = root node has children from both up and down directions
 *
 * Cycles are detected via a visited set and halted without error.
 * maxDepth defaults to 10.
 *
 * Throws NOT_FOUND if rootId is not present in state.tasks.
 */
export function buildDepTree(
  state: State,
  rootId: string,
  direction: DepDirection,
  maxDepth: number = DEFAULT_MAX_DEPTH,
): DepTreeNode {
  const rootTask = state.tasks[rootId];
  if (!rootTask) {
    throw new TsqError("NOT_FOUND", `task not found: ${rootId}`, 1);
  }

  const dependentsByBlocker = buildDependentsByBlocker(state.deps);

  if (direction === "both") {
    const visitedUp = new Set<string>([rootId]);
    const visitedDown = new Set<string>([rootId]);
    const upChildren = walkUp(state, rootId, 1, maxDepth, visitedUp, "up");
    const downChildren = walkDown(
      dependentsByBlocker,
      state,
      rootId,
      1,
      maxDepth,
      visitedDown,
      "down",
    );
    return {
      id: rootId,
      task: rootTask,
      direction: "both",
      depth: 0,
      children: [...upChildren, ...downChildren],
    };
  }

  const visited = new Set<string>([rootId]);

  if (direction === "up") {
    return {
      id: rootId,
      task: rootTask,
      direction: "up",
      depth: 0,
      children: walkUp(state, rootId, 1, maxDepth, visited, "up"),
    };
  }

  return {
    id: rootId,
    task: rootTask,
    direction: "down",
    depth: 0,
    children: walkDown(dependentsByBlocker, state, rootId, 1, maxDepth, visited, "down"),
  };
}

function walkUp(
  state: State,
  nodeId: string,
  depth: number,
  maxDepth: number,
  visited: Set<string>,
  direction: DepDirection,
): DepTreeNode[] {
  if (depth > maxDepth) {
    return [];
  }
  const blockers = state.deps[nodeId] ?? [];
  const nodes: DepTreeNode[] = [];
  for (const blockerId of blockers) {
    if (visited.has(blockerId)) {
      continue;
    }
    const blockerTask = state.tasks[blockerId];
    if (!blockerTask) {
      continue;
    }
    visited.add(blockerId);
    nodes.push({
      id: blockerId,
      task: blockerTask,
      direction,
      depth,
      children: walkUp(state, blockerId, depth + 1, maxDepth, visited, direction),
    });
  }
  return nodes;
}

function walkDown(
  dependentsByBlocker: Map<string, string[]>,
  state: State,
  nodeId: string,
  depth: number,
  maxDepth: number,
  visited: Set<string>,
  direction: DepDirection,
): DepTreeNode[] {
  if (depth > maxDepth) {
    return [];
  }
  const dependents = dependentsByBlocker.get(nodeId) ?? [];
  const nodes: DepTreeNode[] = [];
  for (const dependentId of dependents) {
    if (visited.has(dependentId)) {
      continue;
    }
    const dependentTask = state.tasks[dependentId];
    if (!dependentTask) {
      continue;
    }
    visited.add(dependentId);
    nodes.push({
      id: dependentId,
      task: dependentTask,
      direction,
      depth,
      children: walkDown(
        dependentsByBlocker,
        state,
        dependentId,
        depth + 1,
        maxDepth,
        visited,
        direction,
      ),
    });
  }
  return nodes;
}

/** Build a reverse index: blocker -> list of dependents (children). */
export function buildDependentsByBlocker(deps: State["deps"]): Map<string, string[]> {
  const map = new Map<string, string[]>();
  for (const [child, blockers] of Object.entries(deps)) {
    for (const blocker of blockers) {
      const list = map.get(blocker);
      if (list) {
        list.push(child);
      } else {
        map.set(blocker, [child]);
      }
    }
  }
  return map;
}
