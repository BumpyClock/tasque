import { createHash } from "node:crypto";

import type { State } from "../types";

const CHILD_SEGMENT_RE = /^\d+$/;

export const makeRootId = (title: string, nonce?: string): string => {
  const seed = nonce ? `${title}::${nonce}` : title;
  const hash = createHash("sha256").update(seed).digest("hex").slice(0, 6);
  return `tsq-${hash}`;
};

export const nextChildId = (state: State, parentId: string): string => {
  const prefix = `${parentId}.`;
  let maxChild = state.child_counters[parentId] ?? 0;

  for (const taskId of Object.keys(state.tasks)) {
    if (!taskId.startsWith(prefix)) {
      continue;
    }
    const segment = taskId.slice(prefix.length);
    if (!CHILD_SEGMENT_RE.test(segment)) {
      continue;
    }
    const childIndex = Number.parseInt(segment, 10);
    if (childIndex > maxChild) {
      maxChild = childIndex;
    }
  }

  return `${parentId}.${maxChild + 1}`;
};
