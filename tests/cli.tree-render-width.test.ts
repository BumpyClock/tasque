import { describe, expect, it } from "bun:test";
import { renderTaskTree } from "../src/cli/render";
import type { TaskTreeNode } from "../src/types";

function makeNode(title: string): TaskTreeNode {
  return {
    task: {
      id: "tsq-root1",
      kind: "feature",
      title,
      notes: [],
      status: "open",
      priority: 2,
      assignee: "ada",
      labels: [],
      created_at: "2026-02-17T00:00:00.000Z",
      updated_at: "2026-02-17T00:00:00.000Z",
    },
    blockers: ["tsq-block1", "tsq-block2"],
    dependents: ["tsq-dep1"],
    children: [],
  };
}

function renderTreeLines(nodes: TaskTreeNode[], width: number): string[] {
  return renderTaskTree(nodes, { width });
}

const hasEllipsis = (value: string): boolean => value.includes("…") || value.includes("...");

describe("tree renderer width tiers", () => {
  it("renders metadata inline at wide widths", () => {
    const lines = renderTreeLines(
      [
        makeNode(
          "Implement renderer width tiers with enough content to avoid accidental formatting edge cases",
        ),
      ],
      120,
    );
    const nodeLines = lines.slice(0, -1);

    expect(nodeLines.length).toBe(1);
    expect(nodeLines[0]).toContain("[p2 @ada]");
    expect(nodeLines[0]).toContain("{blocks-on: tsq-block1,tsq-block2 | unblocks: tsq-dep1}");
  });

  it("renders dependency metadata on a second aligned line at medium widths when present", () => {
    const lines = renderTreeLines(
      [
        makeNode(
          "Implement renderer width tiers with enough content to avoid accidental formatting edge cases",
        ),
      ],
      100,
    );
    const nodeLines = lines.slice(0, -1);

    expect(nodeLines.length).toBeGreaterThanOrEqual(2);
    expect(nodeLines[0]).not.toContain("blocks-on:");
    expect(nodeLines[1]).toContain("blocks-on: tsq-block1,tsq-block2");
    expect(nodeLines[1]).toContain("unblocks: tsq-dep1");
    // biome-ignore lint/suspicious/noControlCharactersInRegex: stripping ANSI escape codes
    const stripped = (nodeLines[1] ?? "").replace(/\u001b\[[0-9;]*m/gu, "");
    expect(/^\s+\{/.test(stripped)).toBe(true);
  });

  it("truncates title with ellipsis at narrow widths and moves metadata to following lines", () => {
    const lines = renderTreeLines(
      [
        makeNode(
          "Implement renderer width tiers with enough content to force truncation at narrow terminal widths",
        ),
      ],
      80,
    );
    const nodeLines = lines.slice(0, -1);

    expect(nodeLines.length).toBeGreaterThanOrEqual(2);
    expect(hasEllipsis(nodeLines[0] ?? "")).toBe(true);
    expect(nodeLines[0]).not.toContain("[p2 @ada]");
    expect(nodeLines[0]).not.toContain("blocks-on:");
    expect(nodeLines.slice(1).some((line) => line.includes("[p2 @ada]"))).toBe(true);
    expect(
      nodeLines.slice(1).some((line) => line.includes("blocks-on: tsq-block1,tsq-block2")),
    ).toBe(true);
  });
});
