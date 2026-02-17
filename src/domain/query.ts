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
  "title",
  "status",
  "kind",
  "priority",
  "assignee",
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
 *   - `bare words`           — match title substring (field = "title")
 *
 * Supported fields: id, title, status, kind, priority, assignee, parent, label, ready.
 * Unknown fields are treated as title substring matches.
 *
 * Consecutive bare words are combined into a single title term.
 */
export function parseQuery(q: string): QueryFilter {
  const tokens = tokenize(q.trim());
  const terms: QueryTerm[] = [];
  const bareWords: string[] = [];

  for (const token of tokens) {
    const match = FIELD_TERM_RE.exec(token);
    if (match) {
      if (bareWords.length > 0) {
        terms.push({ field: "title", value: bareWords.join(" "), negated: false });
        bareWords.length = 0;
      }
      const negated = match[1] === "-";
      const rawField = match[2] ?? "";
      const rawValue = match[3] ?? "";
      const value = rawValue.replace(/^"(.*)"$/u, "$1");
      const field = SUPPORTED_FIELDS.has(rawField) ? rawField : "title";
      const termValue =
        field === "title" && !SUPPORTED_FIELDS.has(rawField) ? `${rawField}:${value}` : value;
      terms.push({ field, value: termValue, negated });
    } else {
      const unquoted = token.replace(/^"(.*)"$/u, "$1");
      bareWords.push(unquoted);
    }
  }

  if (bareWords.length > 0) {
    terms.push({ field: "title", value: bareWords.join(" "), negated: false });
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
    case "title":
      return task.title.toLowerCase().includes(term.value.toLowerCase());
    case "status":
      return task.status === term.value;
    case "kind":
      return task.kind === term.value;
    case "priority":
      return String(task.priority) === term.value;
    case "assignee":
      return task.assignee === term.value;
    case "parent":
      return task.parent_id === term.value;
    case "label":
      return task.labels.some((l) => l.toLowerCase() === term.value.toLowerCase());
    case "ready":
      return isReady(state, task.id) === (term.value === "true");
    default:
      return task.title.toLowerCase().includes(term.value.toLowerCase());
  }
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

    const spaceIdx = input.indexOf(" ", idx);
    const tabIdx = input.indexOf("\t", idx);
    const nextWhitespace =
      spaceIdx === -1 && tabIdx === -1
        ? -1
        : spaceIdx === -1
          ? tabIdx
          : tabIdx === -1
            ? spaceIdx
            : Math.min(spaceIdx, tabIdx);

    if (nextWhitespace === -1) {
      tokens.push(input.slice(idx));
      break;
    }

    tokens.push(input.slice(idx, nextWhitespace));
    idx = nextWhitespace + 1;
  }

  return tokens;
}
