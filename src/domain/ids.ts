import { randomBytes } from "node:crypto";

import type { State } from "../types";

/**
 * Crockford base32 alphabet used by ULID.
 * 8 chars at 5 bits each = 40 bits of entropy (~1 trillion combinations).
 */
const CROCKFORD = "0123456789abcdefghjkmnpqrstvwxyz";

/**
 * Generate a root task ID using ULID short-form encoding.
 * Produces `tsq-<8 crockford base32 chars>` with 40 bits of randomness.
 *
 * Parameters are accepted for backward compatibility but ignored;
 * all IDs are now random to maximize entropy.
 */
export const makeRootId = (_title?: string, _nonce?: string): string => {
  const bytes = randomBytes(5); // 40 bits
  let id = "";
  let bits = 0;
  let acc = 0;
  for (const byte of bytes) {
    acc = (acc << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      bits -= 5;
      id += CROCKFORD[(acc >> bits) & 0x1f];
    }
  }
  return `tsq-${id}`;
};

// child_counters[parentId] is maintained by the projector during task.created
// event application (setChildCounter), so it is always authoritative and we
// do not need to scan all tasks to find the max child index.
export const nextChildId = (state: State, parentId: string): string => {
  const maxChild = state.child_counters[parentId] ?? 0;
  return `${parentId}.${maxChild + 1}`;
};
