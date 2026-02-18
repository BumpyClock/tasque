import { afterEach, describe, expect, it } from "bun:test";
import { join } from "node:path";
import { cleanupRepos, makeRepo as makeRepoBase, okData, runJson as runJsonBase } from "./helpers";

const VALID_SPEC_MARKDOWN = `# Feature Spec

## Overview
Implement guarded claim flow.

## Constraints / Non-goals
- Local-only validation.
- No multi-machine workflow.

## Interfaces (CLI/API)
- \`tsq spec check <id>\`
- \`tsq update <id> --claim --require-spec\`

## Data model / schema changes
- task spec metadata fields are used for validation.

## Acceptance criteria
- claims can enforce valid/current specs.

## Test plan
- Exercise missing, drifted, and valid spec claim paths.
`;

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-claim-");
}

afterEach(cleanupRepos);

async function runJson(repoDir: string, args: string[], actor = "claim-test") {
  return runJsonBase(repoDir, args, actor);
}

async function initAndCreate(repo: string, title = "Claim test task"): Promise<string> {
  await runJson(repo, ["init"]);
  const created = okData<{ task: { id: string } }>(
    (await runJson(repo, ["create", title])).envelope,
  );
  return created.task.id;
}

async function attachValidSpec(repo: string, taskId: string): Promise<void> {
  await runJson(repo, ["spec", "attach", taskId, "--text", VALID_SPEC_MARKDOWN]);
}

describe("cli claim transitions", () => {
  // ── Happy paths ──────────────────────────────────────────────────────

  it("claim on open task transitions status to in_progress", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);

    const claimed = await runJson(repo, ["update", taskId, "--claim"]);
    expect(claimed.exitCode).toBe(0);
    expect(claimed.envelope.ok).toBe(true);

    const shown = okData<{
      task: { id: string; status: string; assignee?: string };
    }>((await runJson(repo, ["show", taskId])).envelope);
    expect(shown.task.status).toBe("in_progress");
    expect(typeof shown.task.assignee).toBe("string");
  });

  it("claim on open task sets default actor as assignee", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);

    await runJson(repo, ["update", taskId, "--claim"], "default-actor");

    const shown = okData<{ task: { id: string; assignee?: string } }>(
      (await runJson(repo, ["show", taskId])).envelope,
    );
    expect(shown.task.assignee).toBe("default-actor");
  });

  it("claim with explicit --assignee sets that assignee", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);

    const claimed = await runJson(repo, ["update", taskId, "--claim", "--assignee", "alice"]);
    expect(claimed.exitCode).toBe(0);

    const shown = okData<{ task: { id: string; assignee?: string } }>(
      (await runJson(repo, ["show", taskId])).envelope,
    );
    expect(shown.task.assignee).toBe("alice");
  });

  it("claim preserves in_progress status when already in_progress", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);

    // First set to in_progress manually
    await runJson(repo, ["update", taskId, "--status", "in_progress"]);

    const claimed = await runJson(repo, ["update", taskId, "--claim", "--assignee", "bob"]);
    expect(claimed.exitCode).toBe(0);

    const shown = okData<{
      task: { id: string; status: string; assignee?: string };
    }>((await runJson(repo, ["show", taskId])).envelope);
    expect(shown.task.status).toBe("in_progress");
    expect(shown.task.assignee).toBe("bob");
  });

  // ── Rejection: invalid statuses ──────────────────────────────────────

  it("claim on closed task is rejected with INVALID_STATUS", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);

    await runJson(repo, ["close", taskId]);

    const claimed = await runJson(repo, ["update", taskId, "--claim"]);
    expect(claimed.exitCode).toBe(1);
    expect(claimed.envelope.ok).toBe(false);
    expect(claimed.envelope.error?.code).toBe("INVALID_STATUS");
    expect(claimed.envelope.error?.message).toContain("closed");
  });

  it("claim on canceled task is rejected with INVALID_STATUS", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);

    await runJson(repo, ["update", taskId, "--status", "canceled"]);

    const claimed = await runJson(repo, ["update", taskId, "--claim"]);
    expect(claimed.exitCode).toBe(1);
    expect(claimed.envelope.ok).toBe(false);
    expect(claimed.envelope.error?.code).toBe("INVALID_STATUS");
    expect(claimed.envelope.error?.message).toContain("canceled");
  });

  it("claim on blocked task is rejected with INVALID_STATUS", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    // Create blocker task and the task to be blocked
    const blocker = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Blocker task"])).envelope,
    ).task;
    const blocked = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Blocked task"])).envelope,
    ).task;

    // Add dependency and set blocked status
    await runJson(repo, ["dep", "add", blocked.id, blocker.id]);
    await runJson(repo, ["update", blocked.id, "--status", "blocked"]);

    const claimed = await runJson(repo, ["update", blocked.id, "--claim"]);
    expect(claimed.exitCode).toBe(1);
    expect(claimed.envelope.ok).toBe(false);
    expect(claimed.envelope.error?.code).toBe("INVALID_STATUS");
    expect(claimed.envelope.error?.message).toContain("blocked");
  });

  // ── Rejection: already assigned ──────────────────────────────────────

  it("claim on already-assigned task is rejected with CLAIM_CONFLICT", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);

    // First claim succeeds
    const first = await runJson(repo, ["update", taskId, "--claim", "--assignee", "alice"]);
    expect(first.exitCode).toBe(0);

    // Second claim fails
    const second = await runJson(repo, ["update", taskId, "--claim", "--assignee", "bob"]);
    expect(second.exitCode).toBe(1);
    expect(second.envelope.ok).toBe(false);
    expect(second.envelope.error?.code).toBe("CLAIM_CONFLICT");
    expect(second.envelope.error?.message).toContain("alice");
  });

  // ── Rejection: invalid flag combos ───────────────────────────────────

  it("claim combined with --title is rejected", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);

    const result = await runJson(repo, ["update", taskId, "--claim", "--title", "New Title"]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("claim combined with --status is rejected", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);

    const result = await runJson(repo, ["update", taskId, "--claim", "--status", "in_progress"]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("claim combined with --priority is rejected", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);

    const result = await runJson(repo, ["update", taskId, "--claim", "--priority", "2"]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  // ── Claim gate: --require-spec ───────────────────────────────────────

  it("claim with --require-spec rejects tasks without an attached spec", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);

    const claimed = await runJson(repo, ["update", taskId, "--claim", "--require-spec"]);
    expect(claimed.exitCode).toBeGreaterThan(0);
    expect(claimed.envelope.ok).toBe(false);
    expect(claimed.envelope.error?.code).toBe("SPEC_VALIDATION_FAILED");
    const details = claimed.envelope.error?.details as
      | { diagnostics?: Array<{ code?: string }> }
      | undefined;
    const codes = (details?.diagnostics ?? []).map((diagnostic) => diagnostic.code);
    expect(codes).toContain("SPEC_NOT_ATTACHED");
  });

  it("claim with --require-spec rejects drifted specs", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);
    await attachValidSpec(repo, taskId);
    await Bun.write(
      join(repo, ".tasque", "specs", taskId, "spec.md"),
      `${VALID_SPEC_MARKDOWN}\n<!-- drift -->\n`,
    );

    const claimed = await runJson(repo, ["update", taskId, "--claim", "--require-spec"]);
    expect(claimed.exitCode).toBeGreaterThan(0);
    expect(claimed.envelope.ok).toBe(false);
    expect(claimed.envelope.error?.code).toBe("SPEC_VALIDATION_FAILED");
    const details = claimed.envelope.error?.details as
      | { diagnostics?: Array<{ code?: string }> }
      | undefined;
    const codes = (details?.diagnostics ?? []).map((diagnostic) => diagnostic.code);
    expect(codes).toContain("SPEC_FINGERPRINT_DRIFT");
  });

  it("claim with --require-spec rejects specs missing required sections", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);
    await runJson(repo, [
      "spec",
      "attach",
      taskId,
      "--text",
      "# Thin Spec\n\n## Overview\nOnly overview\n",
    ]);

    const claimed = await runJson(repo, ["update", taskId, "--claim", "--require-spec"]);
    expect(claimed.exitCode).toBeGreaterThan(0);
    expect(claimed.envelope.ok).toBe(false);
    expect(claimed.envelope.error?.code).toBe("SPEC_VALIDATION_FAILED");
    const details = claimed.envelope.error?.details as
      | { diagnostics?: Array<{ code?: string }> }
      | undefined;
    const codes = (details?.diagnostics ?? []).map((diagnostic) => diagnostic.code);
    expect(codes).toContain("SPEC_REQUIRED_SECTIONS_MISSING");
  });

  it("claim with --require-spec succeeds when the spec is present and valid", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);
    await attachValidSpec(repo, taskId);

    const claimed = await runJson(
      repo,
      ["update", taskId, "--claim", "--require-spec", "--assignee", "spec-owner"],
      "claim-with-spec",
    );

    expect(claimed.exitCode).toBe(0);
    expect(claimed.envelope.ok).toBe(true);
    const shown = okData<{ task: { status: string; assignee?: string } }>(
      (await runJson(repo, ["show", taskId])).envelope,
    );
    expect(shown.task.status).toBe("in_progress");
    expect(shown.task.assignee).toBe("spec-owner");
  });

  // ── Task state integrity after claim ─────────────────────────────────

  it("claim does not alter task title, priority, or labels", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Preserve fields", "-p", "2"])).envelope,
    ).task;

    // Add a label first
    await runJson(repo, ["label", "add", created.id, "regression"]);

    const beforeClaim = okData<{
      task: { id: string; title: string; priority: number; labels: string[] };
    }>((await runJson(repo, ["show", created.id])).envelope);

    await runJson(repo, ["update", created.id, "--claim", "--assignee", "tester"]);

    const afterClaim = okData<{
      task: {
        id: string;
        title: string;
        priority: number;
        labels: string[];
        status: string;
        assignee?: string;
      };
    }>((await runJson(repo, ["show", created.id])).envelope);

    expect(afterClaim.task.title).toBe(beforeClaim.task.title);
    expect(afterClaim.task.priority).toBe(beforeClaim.task.priority);
    expect(afterClaim.task.labels).toEqual(beforeClaim.task.labels);
    expect(afterClaim.task.status).toBe("in_progress");
    expect(afterClaim.task.assignee).toBe("tester");
  });

  // ── Claim event appears in history ───────────────────────────────────

  it("claim emits a task.claimed event in history", async () => {
    const repo = await makeRepo();
    const taskId = await initAndCreate(repo);

    await runJson(repo, ["update", taskId, "--claim", "--assignee", "historian"]);

    const history = await runJson(repo, ["history", taskId, "--type", "task.claimed"]);
    expect(history.exitCode).toBe(0);

    const data = okData<{
      events: Array<{ type: string; payload: Record<string, unknown> }>;
    }>(history.envelope);
    expect(data.events.length).toBe(1);
    const first = data.events.at(0);
    expect(first).toBeDefined();
    expect(first?.type).toBe("task.claimed");
    expect(first?.payload.assignee).toBe("historian");
  });

  // ── NOT_FOUND ────────────────────────────────────────────────────────

  it("claim on non-existent task returns NOT_FOUND", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const result = await runJson(repo, ["update", "tsq-doesnotexist", "--claim"]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("TASK_NOT_FOUND");
  });
});
