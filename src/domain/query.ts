import type { State, Task } from "../types";
import { isReady } from "./validate";

/**
 * A single parsed search term with optional field qualifier and negation.
 *
 * Example terms produced by parseQuery:
 *   - `status:open`        → { field: "status", value: "open", negated: false }
 *   - `-label:bug`         → { field: "label",  value: "bug",  negated: true  }
 *   - `login`              → { field: "title",  value: "login", negated: false }
 */
export interface QueryTerm {
  field: string;
  value: string;
  negated: boolean;
}

/**
 * A structured query filter produced by parseQuery.
 * All terms are combined with implicit AND logic in evaluateQuery.
 */
export interface QueryFilter {
  terms: QueryTerm[];
}

const FIELD_TERM_RE = /^(-?)(\w+):(.+)$/u;
const SUPPORTED_FIELDS = new Set([
  "id",
  "text",
  "title",
  "description",
  "notes",
  "status",
  "kind",
  "priority",
  "assignee",
  "external_ref",
  "parent",
  "label",
  "ready",
]);

/**
 * Parse a query string into structured terms.
 *
 * Syntax:
 *   - `field:value`          — match field equals value
 *   - `field:"quoted value"` — match field with spaces
 *   - `-field:value`         — negation
 *   - `bare words`           — match title/description/notes text (field = "text")
 *
 * Supported fields: id, text, title, description, notes, status, kind, priority, assignee, external_ref, parent, label, ready.
 * Unknown fields are treated as text matches.
 *
 * Consecutive bare words are combined into a single text term.
 */
export function parseQuery(q: string): QueryFilter {
  const tokens = tokenize(q.trim());
  const terms: QueryTerm[] = [];
  const bareWords: string[] = [];

  for (const token of tokens) {
    const match = FIELD_TERM_RE.exec(token);
    if (match) {
      if (bareWords.length > 0) {
        terms.push({ field: "text", value: bareWords.join(" "), negated: false });
        bareWords.length = 0;
      }
      const negated = match[1] === "-";
      const rawField = match[2] ?? "";
      const rawValue = match[3] ?? "";
      const value = rawValue.replace(/^"(.*)"$/u, "$1");
      const field = SUPPORTED_FIELDS.has(rawField) ? rawField : "text";
      const termValue =
        field === "text" && !SUPPORTED_FIELDS.has(rawField) ? `${rawField}:${value}` : value;
      terms.push({ field, value: termValue, negated });
    } else {
      const unquoted = token.replace(/^"(.*)"$/u, "$1");
      bareWords.push(unquoted);
    }
  }

  if (bareWords.length > 0) {
    terms.push({ field: "text", value: bareWords.join(" "), negated: false });
  }

  return { terms };
}

/**
 * Evaluate a query filter against an array of tasks using implicit AND logic.
 * State is required for ready-state checks.
 */
export function evaluateQuery(tasks: Task[], filter: QueryFilter, state: State): Task[] {
  if (filter.terms.length === 0) {
    return tasks;
  }
  return tasks.filter((task) => matchesAll(task, filter.terms, state));
}

function matchesAll(task: Task, terms: QueryTerm[], state: State): boolean {
  for (const term of terms) {
    const matched = matchTerm(task, term, state);
    if (term.negated ? matched : !matched) {
      return false;
    }
  }
  return true;
}

function matchTerm(task: Task, term: QueryTerm, state: State): boolean {
  switch (term.field) {
    case "id":
      return task.id === term.value || task.id.startsWith(term.value);
    case "text":
      return matchTaskText(task, term.value);
    case "title":
      return task.title.toLowerCase().includes(term.value.toLowerCase());
    case "description":
      return (task.description ?? "").toLowerCase().includes(term.value.toLowerCase());
    case "notes":
      return (task.notes ?? []).some((note) =>
        note.text.toLowerCase().includes(term.value.toLowerCase()),
      );
    case "status":
      return task.status === term.value;
    case "kind":
      return task.kind === term.value;
    case "priority":
      return String(task.priority) === term.value;
    case "assignee":
      return task.assignee === term.value;
    case "external_ref":
      return task.external_ref === term.value;
    case "parent":
      return task.parent_id === term.value;
    case "label":
      return task.labels.some((l) => l.toLowerCase() === term.value.toLowerCase());
    case "ready":
      return isReady(state, task.id) === (term.value === "true");
    default:
      return matchTaskText(task, term.value);
  }
}

function matchTaskText(task: Task, value: string): boolean {
  const needle = value.toLowerCase();
  if (task.title.toLowerCase().includes(needle)) {
    return true;
  }
  if ((task.description ?? "").toLowerCase().includes(needle)) {
    return true;
  }
  return (task.notes ?? []).some((note) => note.text.toLowerCase().includes(needle));
}

function tokenize(input: string): string[] {
  const tokens: string[] = [];
  let idx = 0;

  while (idx < input.length) {
    if (input[idx] === " " || input[idx] === "\t") {
      idx += 1;
      continue;
    }

    if (input[idx] === '"') {
      const end = input.indexOf('"', idx + 1);
      if (end === -1) {
        tokens.push(input.slice(idx + 1));
        break;
      }
      tokens.push(input.slice(idx, end + 1));
      idx = end + 1;
      continue;
    }

    // Scan a non-whitespace token, handling embedded quoted strings
    // (e.g. title:"my task" should be one token).
    let end = idx;
    while (end < input.length && input[end] !== " " && input[end] !== "\t") {
      if (input[end] === '"') {
        const closeQuote = input.indexOf('"', end + 1);
        end = closeQuote === -1 ? input.length : closeQuote + 1;
      } else {
        end += 1;
      }
    }
    tokens.push(input.slice(idx, end));
    idx = end;
  }

  return tokens;
}
