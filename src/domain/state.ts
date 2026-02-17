import type { State } from "../types";

export const createEmptyState = (): State => ({
  tasks: {},
  deps: {},
  links: {},
  child_counters: {},
  created_order: [],
  applied_events: 0,
});
