import { normalizeStatus } from "../app/runtime";
import type { ListFilter } from "../app/service";
import type { DepDirection } from "../domain/dep-tree";
import { TsqError } from "../errors";
import type { SkillTarget } from "../skills/types";
import type { DependencyType, PlanningState, RelationType, TaskKind, TaskStatus } from "../types";

export interface GlobalOpts {
  json?: boolean;
  exactId?: boolean;
}

export interface InitCommandOptions {
  installSkill?: boolean;
  uninstallSkill?: boolean;
  skillTargets?: string;
  skillName?: string;
  forceSkillOverwrite?: boolean;
  skillDirClaude?: string;
  skillDirCodex?: string;
  skillDirCopilot?: string;
  skillDirOpencode?: string;
}

export interface ListCommandOptions {
  status?: string;
  assignee?: string;
  unassigned?: boolean;
  externalRef?: string;
  discoveredFrom?: string;
  kind?: string;
  label?: string;
  labelAny?: string[];
  createdAfter?: string;
  updatedAfter?: string;
  closedAfter?: string;
  id?: string[];
  tree?: boolean;
  full?: boolean;
  planning?: string;
  depType?: string;
  depDirection?: string;
}

export interface StaleCommandOptions {
  days?: string;
  status?: string;
  assignee?: string;
  limit?: string;
}

export interface CreateCommandOptions {
  kind?: string;
  priority?: string;
  parent?: string;
  description?: string;
  externalRef?: string;
  discoveredFrom?: string;
  planning?: string;
  needsPlanning?: boolean;
  id?: string;
  bodyFile?: string;
}

export interface UpdateCommandOptions {
  title?: string;
  description?: string;
  clearDescription?: boolean;
  externalRef?: string;
  discoveredFrom?: string;
  clearDiscoveredFrom?: boolean;
  clearExternalRef?: boolean;
  status?: string;
  priority?: string;
  claim?: boolean;
  assignee?: string;
  requireSpec?: boolean;
  planning?: string;
}

export interface SpecAttachCommandOptions {
  file?: string;
  stdin?: boolean;
  text?: string;
  force?: boolean;
}

export interface WatchCommandOptions {
  interval: string;
  status: string;
  assignee?: string;
  tree?: boolean;
  once?: boolean;
}

export const TREE_DEFAULT_STATUSES: TaskStatus[] = ["open", "in_progress"];

export function asOptionalString(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

export function parseKind(raw: string): TaskKind {
  if (raw === "task" || raw === "feature" || raw === "epic") {
    return raw;
  }
  throw new TsqError("VALIDATION_ERROR", "kind must be task|feature|epic", 1);
}

export function parseRelationType(raw: string): RelationType {
  if (
    raw === "relates_to" ||
    raw === "replies_to" ||
    raw === "duplicates" ||
    raw === "supersedes"
  ) {
    return raw;
  }
  throw new TsqError(
    "VALIDATION_ERROR",
    "relation type must be relates_to|replies_to|duplicates|supersedes",
    1,
  );
}

export function parsePlanningState(raw: string): PlanningState {
  if (raw === "needs_planning" || raw === "planned") {
    return raw;
  }
  throw new TsqError("VALIDATION_ERROR", "planning state must be needs_planning|planned", 1);
}

export function validateExplicitId(raw: string): string {
  const trimmed = raw.trim();
  if (!/^tsq-[0-9a-hjkmnp-tv-z]{8}$/.test(trimmed)) {
    throw new TsqError(
      "VALIDATION_ERROR",
      "explicit --id must match tsq-<8 crockford base32 chars>",
      1,
    );
  }
  return trimmed;
}

export function parseLane(raw: string): "planning" | "coding" {
  if (raw === "planning" || raw === "coding") {
    return raw;
  }
  throw new TsqError("VALIDATION_ERROR", "lane must be planning|coding", 1);
}

export function parseSkillTargets(raw: string): SkillTarget[] {
  const tokens = raw
    .split(",")
    .map((entry) => entry.trim().toLowerCase())
    .filter((entry) => entry.length > 0);
  if (tokens.length === 0) {
    throw new TsqError("VALIDATION_ERROR", "skill targets must not be empty", 1);
  }

  const validTargets: SkillTarget[] = ["claude", "codex", "copilot", "opencode"];
  if (tokens.includes("all")) {
    return validTargets;
  }

  const unique: SkillTarget[] = [];
  for (const token of tokens) {
    if (!isSkillTarget(token)) {
      throw new TsqError(
        "VALIDATION_ERROR",
        "skill targets must be comma-separated values of claude,codex,copilot,opencode,all",
        1,
      );
    }
    if (!unique.includes(token)) {
      unique.push(token);
    }
  }
  return unique;
}

export function parseDepDirection(raw?: string): DepDirection | undefined {
  if (!raw) return undefined;
  if (raw === "up" || raw === "down" || raw === "both") return raw;
  throw new TsqError("VALIDATION_ERROR", "direction must be up|down|both", 1);
}

export function parseDependencyType(raw: string): DependencyType {
  if (raw === "blocks" || raw === "starts_after") {
    return raw;
  }
  throw new TsqError("VALIDATION_ERROR", "dependency type must be blocks|starts_after", 1);
}

export function parseDepFilterDirection(raw: string): "in" | "out" | "any" {
  if (raw === "in" || raw === "out" || raw === "any") {
    return raw;
  }
  throw new TsqError("VALIDATION_ERROR", "dep-direction must be in|out|any", 1);
}

export function parseNonNegativeInt(raw: string, field: string): number {
  const trimmed = raw.trim();
  if (!/^\d+$/u.test(trimmed)) {
    throw new TsqError("VALIDATION_ERROR", `${field} must be an integer >= 0`, 1);
  }
  return Number.parseInt(trimmed, 10);
}

export function parsePositiveInt(raw: string, field: string, min: number, max: number): number {
  const value = parseNonNegativeInt(raw, field);
  if (value < min || value > max) {
    throw new TsqError("VALIDATION_ERROR", `${field} must be between ${min} and ${max}`, 1);
  }
  return value;
}

export function parseListFilter(
  options: ListCommandOptions,
  unassignedFlag = false,
  hasAssigneeFlag = false,
): ListFilter {
  const filter: ListFilter = {};
  if (options.status) {
    filter.statuses = [normalizeStatus(options.status)];
  }
  if (unassignedFlag && hasAssigneeFlag) {
    throw new TsqError("VALIDATION_ERROR", "cannot combine --assignee with --unassigned", 1);
  }
  const assignee = asOptionalString(options.assignee);
  if (assignee) {
    filter.assignee = assignee;
  }
  const externalRef = asOptionalString(options.externalRef);
  if (externalRef) {
    filter.externalRef = externalRef;
  }
  const discoveredFrom = asOptionalString(options.discoveredFrom);
  if (discoveredFrom) {
    filter.discoveredFrom = discoveredFrom;
  }
  if (options.kind) {
    filter.kind = parseKind(options.kind);
  }
  const label = asOptionalString(options.label);
  if (label) {
    filter.label = label;
  }
  if ((options.labelAny?.length ?? 0) > 0) {
    filter.labelAny = uniqueSorted(options.labelAny ?? []);
  }
  if (options.createdAfter) {
    filter.createdAfter = parseIsoTimestamp(options.createdAfter, "created-after");
  }
  if (options.updatedAfter) {
    filter.updatedAfter = parseIsoTimestamp(options.updatedAfter, "updated-after");
  }
  if (options.closedAfter) {
    filter.closedAfter = parseIsoTimestamp(options.closedAfter, "closed-after");
  }
  if (unassignedFlag) {
    filter.unassigned = true;
  }
  if ((options.id?.length ?? 0) > 0) {
    filter.ids = uniqueSorted(options.id ?? []);
  }
  if (options.planning) {
    filter.planning_state = parsePlanningState(options.planning);
  }
  if (options.depType) {
    filter.depType = parseDependencyType(options.depType);
    filter.depDirection = options.depDirection
      ? parseDepFilterDirection(options.depDirection)
      : "any";
  } else if (options.depDirection) {
    throw new TsqError("VALIDATION_ERROR", "--dep-direction requires --dep-type", 1);
  }
  return filter;
}

export function applyTreeDefaults(filter: ListFilter, options: ListCommandOptions): ListFilter {
  if (options.full || filter.statuses) {
    return filter;
  }
  return {
    ...filter,
    statuses: [...TREE_DEFAULT_STATUSES],
  };
}

export function collectCsvOption(field: string) {
  return (raw: string, previous: string[]): string[] => {
    const next = parseCsvValue(raw, field);
    return uniqueSorted([...(previous ?? []), ...next]);
  };
}

function isSkillTarget(value: string): value is SkillTarget {
  return value === "claude" || value === "codex" || value === "copilot" || value === "opencode";
}

function parseCsvValue(raw: string, field: string): string[] {
  const trimmed = raw.trim();
  if (trimmed.length === 0) {
    throw new TsqError("VALIDATION_ERROR", `--${field} must not be empty`, 1);
  }
  const values = trimmed.split(",").map((entry) => entry.trim());
  if (values.some((entry) => entry.length === 0)) {
    throw new TsqError("VALIDATION_ERROR", `--${field} values must not be empty`, 1);
  }
  return values;
}

function parseIsoTimestamp(raw: string, field: string): string {
  const trimmed = raw.trim();
  if (trimmed.length === 0) {
    throw new TsqError("VALIDATION_ERROR", `--${field} must be a valid ISO timestamp`, 1);
  }
  if (!isIsoTimestamp(trimmed)) {
    throw new TsqError("VALIDATION_ERROR", `--${field} must be a valid ISO timestamp`, 1);
  }
  const value = Date.parse(trimmed);
  if (Number.isNaN(value)) {
    throw new TsqError("VALIDATION_ERROR", `--${field} must be a valid ISO timestamp`, 1);
  }
  return new Date(value).toISOString();
}

function isIsoTimestamp(value: string): boolean {
  return /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})$/u.test(value);
}

function uniqueSorted(values: string[]): string[] {
  return [...new Set(values)].sort((a, b) => a.localeCompare(b));
}
