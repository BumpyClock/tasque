import { afterEach, describe, expect, it } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

interface JsonEnvelope {
  schema_version: number;
  command: string;
  ok: boolean;
  data?: unknown;
  error?: {
    code: string;
    message: string;
    details?: unknown;
  };
}

interface JsonResult {
  exitCode: number;
  stdout: string;
  stderr: string;
  envelope: JsonEnvelope;
}

const repos: string[] = [];
const repoRoot = resolve(import.meta.dir, "..");
const cliEntry = join(repoRoot, "src", "main.ts");

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-cli-concurrency-"));
  repos.push(repo);
  return repo;
}

afterEach(async () => {
  await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
});

function assertEnvelopeShape(value: unknown): asserts value is JsonEnvelope {
  expect(value).toBeObject();
  const envelope = value as Record<string, unknown>;
  expect(envelope.schema_version).toBe(1);
  expect(typeof envelope.command).toBe("string");
  expect(typeof envelope.ok).toBe("boolean");
  if (envelope.ok === true) {
    expect("data" in envelope).toBe(true);
  } else {
    expect("error" in envelope).toBe(true);
  }
}

async function runJson(repoDir: string, args: string[], actor: string): Promise<JsonResult> {
  const proc = Bun.spawn({
    cmd: ["bun", "run", cliEntry, ...args, "--json"],
    cwd: repoDir,
    env: {
      ...process.env,
      TSQ_ACTOR: actor,
    },
    stdout: "pipe",
    stderr: "pipe",
  });

  const [exitCode, stdout, stderr] = await Promise.all([
    proc.exited,
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
  ]);

  const trimmed = stdout.trim();
  expect(trimmed.length > 0).toBe(true);
  const parsed = JSON.parse(trimmed) as unknown;
  assertEnvelopeShape(parsed);

  return {
    exitCode,
    stdout,
    stderr,
    envelope: parsed,
  };
}

function okData<T>(envelope: JsonEnvelope): T {
  expect(envelope.ok).toBe(true);
  return envelope.data as T;
}

describe("cli concurrency", () => {
  it("allows exactly one winner in 5 concurrent claims", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"], "setup");

    const created = await runJson(repo, ["create", "Claim target"], "setup");
    const taskId = okData<{ task: { id: string } }>(created.envelope).task.id;

    const claimAttempts = await Promise.all(
      Array.from({ length: 5 }, (_, idx) =>
        runJson(repo, ["update", taskId, "--claim", "--assignee", `agent-${idx}`], `runner-${idx}`),
      ),
    );

    const winners = claimAttempts.filter((result) => result.envelope.ok);
    const losers = claimAttempts.filter((result) => !result.envelope.ok);

    expect(winners.length).toBe(1);
    expect(losers.length).toBe(4);

    for (const loser of losers) {
      expect(loser.exitCode).toBe(1);
      expect(loser.envelope.error?.code).toBe("CLAIM_CONFLICT");
    }

    const shown = await runJson(repo, ["show", taskId], "verify");
    expect(shown.exitCode).toBe(0);
    const shownTask = okData<{ task: { assignee?: string; status: string } }>(shown.envelope).task;
    expect(typeof shownTask.assignee).toBe("string");
    expect((shownTask.assignee ?? "").startsWith("agent-")).toBe(true);
    expect(shownTask.status).toBe("in_progress");
  });
});
