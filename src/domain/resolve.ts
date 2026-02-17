import { TsqError } from "../errors";
import type { State } from "../types";

export const resolveTaskId = (state: State, raw: string, exactId = false): string => {
  if (exactId) {
    if (state.tasks[raw]) {
      return raw;
    }
    throw new TsqError("TASK_NOT_FOUND", "Task ID not found", 1, { input: raw });
  }

  if (state.tasks[raw]) {
    return raw;
  }

  const matches = Object.keys(state.tasks)
    .filter((taskId) => taskId.startsWith(raw))
    .sort((a, b) => a.localeCompare(b));

  if (matches.length === 1) {
    return matches[0] as string;
  }

  if (matches.length === 0) {
    throw new TsqError("TASK_NOT_FOUND", "Task ID not found", 1, { input: raw });
  }

  throw new TsqError("TASK_ID_AMBIGUOUS", "Task ID is ambiguous", 1, {
    input: raw,
    candidates: matches,
  });
};
