import { describe, expect, it } from "bun:test";
import { makeRootId, nextChildId } from "../src/domain/ids";
import type { State } from "../src/types";

describe("makeRootId", () => {
  it("produces tsq- prefix with 8-char crockford base32 suffix", () => {
    const id = makeRootId();
    expect(id.startsWith("tsq-")).toBe(true);
    const suffix = id.slice(4);
    expect(suffix).toHaveLength(8);
    // Crockford base32 alphabet: 0-9 a-h j-k m-n p-t v-x y-z (no i l o u)
    expect(/^[0-9a-hjkmnp-tv-z]{8}$/u.test(suffix)).toBe(true);
  });

  it("generates unique IDs across multiple calls", () => {
    const ids = new Set<string>();
    for (let i = 0; i < 100; i += 1) {
      ids.add(makeRootId());
    }
    expect(ids.size).toBe(100);
  });

  it("accepts legacy (title, nonce) params without breaking", () => {
    const id = makeRootId("some title", "some-nonce");
    expect(id.startsWith("tsq-")).toBe(true);
    expect(id.slice(4)).toHaveLength(8);
  });

  it("IDs are 12 chars total (tsq- prefix + 8 suffix)", () => {
    const id = makeRootId();
    expect(id).toHaveLength(12);
  });
});

describe("nextChildId", () => {
  it("returns parent.1 when no children exist", () => {
    const state: State = {
      tasks: {},
      deps: {},
      links: {},
      child_counters: {},
      created_order: [],
      applied_events: 0,
    };
    expect(nextChildId(state, "tsq-abc123")).toBe("tsq-abc123.1");
  });

  it("returns parent.N+1 based on child_counters", () => {
    const state: State = {
      tasks: {},
      deps: {},
      links: {},
      child_counters: { "tsq-abc123": 3 },
      created_order: [],
      applied_events: 0,
    };
    expect(nextChildId(state, "tsq-abc123")).toBe("tsq-abc123.4");
  });
});
