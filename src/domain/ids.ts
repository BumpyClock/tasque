import { createHash } from "node:crypto";

import type { State } from "../types";

export const makeRootId = (title: string, nonce?: string): string => {
  const seed = nonce ? `${title}::${nonce}` : title;
  const hash = createHash("sha256").update(seed).digest("hex").slice(0, 6);
  return `tsq-${hash}`;
};

// child_counters[parentId] is maintained by the projector during task.created
// event application (setChildCounter), so it is always authoritative and we
// do not need to scan all tasks to find the max child index.
export const nextChildId = (state: State, parentId: string): string => {
  const maxChild = state.child_counters[parentId] ?? 0;
  return `${parentId}.${maxChild + 1}`;
};
