import { TsqError } from "../errors";

const LABEL_PATTERN = /^[a-z0-9:_/\-]+$/u;
const MAX_LABEL_LENGTH = 64;

/**
 * Normalize a label string: lowercase, trim, validate charset [a-z0-9:_/-], max 64 chars.
 * Throws TsqError("VALIDATION_ERROR", ...) on invalid input.
 */
export function normalizeLabel(raw: string): string {
  const trimmed = raw.trim().toLowerCase();
  if (trimmed.length === 0) {
    throw new TsqError("VALIDATION_ERROR", "label must not be empty", 1);
  }
  if (trimmed.length > MAX_LABEL_LENGTH) {
    throw new TsqError(
      "VALIDATION_ERROR",
      `label must not exceed ${MAX_LABEL_LENGTH} characters`,
      1,
    );
  }
  if (!LABEL_PATTERN.test(trimmed)) {
    throw new TsqError("VALIDATION_ERROR", "label must only contain characters [a-z0-9:_/-]", 1);
  }
  return trimmed;
}

/**
 * Add a label to the current set. Returns a new sorted, deduplicated array.
 */
export function addLabel(current: string[], label: string): string[] {
  const normalized = normalizeLabel(label);
  const set = new Set(current);
  set.add(normalized);
  return [...set].sort();
}

/**
 * Remove a label from the current set. Returns a new array without that label.
 * Throws TsqError("NOT_FOUND", ...) if label not present.
 */
export function removeLabel(current: string[], label: string): string[] {
  const normalized = normalizeLabel(label);
  if (!current.includes(normalized)) {
    throw new TsqError("NOT_FOUND", `label not found: ${normalized}`, 1);
  }
  return current.filter((entry) => entry !== normalized);
}
