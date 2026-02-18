import type { DependencyEdge, DependencyType } from "../types";

const DEPENDENCY_TYPES: readonly DependencyType[] = ["blocks", "starts_after"];
const DEFAULT_DEP_TYPE: DependencyType = "blocks";

export function normalizeDependencyType(value: unknown): DependencyType | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  return DEPENDENCY_TYPES.includes(value as DependencyType) ? (value as DependencyType) : undefined;
}

export function toDependencyEdge(value: unknown): DependencyEdge | undefined {
  if (typeof value === "string" && value.length > 0) {
    return { blocker: value, dep_type: DEFAULT_DEP_TYPE };
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const candidate = value as Partial<DependencyEdge> & Record<string, unknown>;
  if (typeof candidate.blocker !== "string" || candidate.blocker.length === 0) {
    return undefined;
  }
  const depType = normalizeDependencyType(candidate.dep_type) ?? DEFAULT_DEP_TYPE;
  return { blocker: candidate.blocker, dep_type: depType };
}

export function normalizeDependencyEdges(value: unknown): DependencyEdge[] {
  if (!Array.isArray(value)) {
    return [];
  }
  const seen = new Set<string>();
  const normalized: DependencyEdge[] = [];
  for (const entry of value) {
    const edge = toDependencyEdge(entry);
    if (!edge) {
      continue;
    }
    const key = edgeKey(edge.blocker, edge.dep_type);
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    normalized.push(edge);
  }
  return normalized;
}

export function edgeKey(blocker: string, depType: DependencyType): string {
  return `${blocker}\u0000${depType}`;
}
