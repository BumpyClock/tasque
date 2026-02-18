import { TsqError } from "../errors";
import type { DependencyType, State, Task } from "../types";
import { normalizeDependencyEdges } from "./deps";

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
  dep_type?: DependencyType;
  children: DepTreeNode[];
}

interface DependentEdge {
  id: string;
  dep_type: DependencyType;
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
  const blockers = normalizeDependencyEdges(state.deps[nodeId] as unknown);
  const nodes: DepTreeNode[] = [];
  for (const edge of blockers) {
    const blockerId = edge.blocker;
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
      dep_type: edge.dep_type,
      children: walkUp(state, blockerId, depth + 1, maxDepth, visited, direction),
    });
  }
  return nodes;
}

function walkDown(
  dependentsByBlocker: Map<string, DependentEdge[]>,
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
  for (const dependentEdge of dependents) {
    const dependentId = dependentEdge.id;
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
      dep_type: dependentEdge.dep_type,
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
export function buildDependentsByBlocker(deps: State["deps"]): Map<string, DependentEdge[]> {
  const map = new Map<string, DependentEdge[]>();
  for (const [child, blockers] of Object.entries(deps)) {
    for (const edge of normalizeDependencyEdges(blockers as unknown)) {
      const list = map.get(edge.blocker);
      const dependent = { id: child, dep_type: edge.dep_type };
      if (list) {
        list.push(dependent);
      } else {
        map.set(edge.blocker, [dependent]);
      }
    }
  }
  return map;
}
