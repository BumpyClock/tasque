import { describe, expect, it } from "bun:test";
import { createInFlightGuard, createLatestGuard } from "../src/data";

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

describe("createInFlightGuard", () => {
  it("skips a call while a fetch is in flight instead of queueing it", async () => {
    let calls = 0;
    const gate = deferred<string>();
    const guarded = createInFlightGuard(() => {
      calls += 1;
      return gate.promise;
    });

    const first = guarded();
    const second = guarded();

    expect(await second).toBeUndefined();
    expect(calls).toBe(1);

    gate.resolve("done");
    expect(await first).toBe("done");
  });

  it("allows the next call after the previous one resolves", async () => {
    let calls = 0;
    const guarded = createInFlightGuard(async () => {
      calls += 1;
      return calls;
    });

    expect(await guarded()).toBe(1);
    expect(await guarded()).toBe(2);
    expect(calls).toBe(2);
  });

  it("releases the guard when the fetcher rejects", async () => {
    let calls = 0;
    const guarded = createInFlightGuard(async () => {
      calls += 1;
      if (calls === 1) {
        throw new Error("boom");
      }
      return "recovered";
    });

    await expect(guarded()).rejects.toThrow("boom");
    expect(await guarded()).toBe("recovered");
  });
});

describe("createLatestGuard", () => {
  it("discards an older response that resolves after a newer request", async () => {
    const guard = createLatestGuard();
    const slow = deferred<string>();
    const fast = deferred<string>();

    const first = guard(() => slow.promise);
    const second = guard(() => fast.promise);

    fast.resolve("new");
    expect(await second).toBe("new");

    slow.resolve("old");
    expect(await first).toBeUndefined();
  });

  it("passes through sequential responses", async () => {
    const guard = createLatestGuard();
    expect(await guard(async () => "a")).toBe("a");
    expect(await guard(async () => "b")).toBe("b");
  });
});
