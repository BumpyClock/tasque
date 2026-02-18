import { afterEach, describe, expect, it } from "bun:test";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
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

interface SpecAttachData {
  task: {
    id: string;
    spec_path?: string;
    spec_fingerprint?: string;
    spec_attached_at?: string;
    spec_attached_by?: string;
  };
  spec: {
    spec_path: string;
    spec_fingerprint: string;
    spec_attached_at: string;
    spec_attached_by: string;
    bytes: number;
  };
}

interface SpecCheckData {
  task_id: string;
  ok: boolean;
  spec: {
    attached: boolean;
    spec_path?: string;
    expected_fingerprint?: string;
    actual_fingerprint?: string;
    bytes?: number;
    required_sections: string[];
    present_sections: string[];
    missing_sections: string[];
  };
  diagnostics: Array<{
    code: string;
    message: string;
    details?: unknown;
  }>;
}

const repos: string[] = [];
const repoRoot = resolve(import.meta.dir, "..");
const cliEntry = join(repoRoot, "src", "main.ts");
const VALID_SPEC_MARKDOWN = `# Feature Spec

## Overview
Deliver durable workflow checks for task specs.

## Constraints / Non-goals
- Local-only validation + claim gating.
- No daemon/sync changes.

## Interfaces (CLI/API)
- \`tsq spec attach <id> ...\`
- \`tsq spec check <id>\`
- \`tsq update <id> --claim --require-spec\`

## Data model / schema changes
- task metadata stores spec path and fingerprint.

## Acceptance criteria
- drift and missing sections are detected.

## Test plan
- Add CLI tests for success, drift, and missing sections.
`;

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-spec-e2e-"));
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

async function runJson(
  repoDir: string,
  args: string[],
  actor = "test-spec",
  stdinText?: string,
): Promise<JsonResult> {
  const proc = Bun.spawn({
    cmd: ["bun", "run", cliEntry, ...args, "--json"],
    cwd: repoDir,
    env: {
      ...process.env,
      TSQ_ACTOR: actor,
    },
    stdin: stdinText === undefined ? "ignore" : "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });

  if (stdinText !== undefined) {
    const stdin = proc.stdin;
    expect(stdin).toBeDefined();
    stdin?.write(stdinText);
    stdin?.end();
  }

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

async function createTask(repo: string, title: string): Promise<string> {
  const created = okData<{ task: { id: string } }>(
    await runJson(repo, ["create", title]).then((result) => result.envelope),
  );
  return created.task.id;
}

function expectSpecMetadata(data: SpecAttachData, taskId: string, actor: string): void {
  const expectedPath = `.tasque/specs/${taskId}/spec.md`;
  expect(data.task.id).toBe(taskId);
  expect(data.task.spec_path).toBe(expectedPath);
  expect(data.spec.spec_path).toBe(expectedPath);
  expect(data.task.spec_fingerprint).toBe(data.spec.spec_fingerprint);
  expect(data.task.spec_attached_at).toBe(data.spec.spec_attached_at);
  expect(data.task.spec_attached_by).toBe(actor);
  expect(data.spec.spec_attached_by).toBe(actor);
  expect(typeof data.spec.spec_fingerprint).toBe("string");
  expect(data.spec.spec_fingerprint.length).toBe(64);
}

describe("cli spec attach", () => {
  it("attaches spec via --text", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const taskId = await createTask(repo, "Spec text task");
    const markdown = "# API Spec\n\nShip via --text";

    const attached = await runJson(repo, ["spec", "attach", taskId, "--text", markdown]);
    expect(attached.exitCode).toBe(0);
    const data = okData<SpecAttachData>(attached.envelope);
    expectSpecMetadata(data, taskId, "test-spec");
    expect(data.spec.bytes).toBe(markdown.length);

    const persisted = await readFile(join(repo, ".tasque", "specs", taskId, "spec.md"), "utf8");
    expect(persisted).toBe(markdown);
  });

  it("attaches spec via --file", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const taskId = await createTask(repo, "Spec file task");
    const sourcePath = join(repo, "incoming-spec.md");
    const markdown = "# File Spec\n\nLoaded from file";
    await writeFile(sourcePath, markdown, "utf8");

    const attached = await runJson(repo, ["spec", "attach", taskId, "--file", sourcePath]);
    expect(attached.exitCode).toBe(0);
    const data = okData<SpecAttachData>(attached.envelope);
    expectSpecMetadata(data, taskId, "test-spec");
    expect(data.spec.bytes).toBe(markdown.length);
    expect(await readFile(join(repo, ".tasque", "specs", taskId, "spec.md"), "utf8")).toBe(
      markdown,
    );
  });

  it("attaches spec via --stdin", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const taskId = await createTask(repo, "Spec stdin task");
    const markdown = "# Stdin Spec\n\nPiped source";

    const attached = await runJson(
      repo,
      ["spec", "attach", taskId, "--stdin"],
      "stdin-actor",
      markdown,
    );
    expect(attached.exitCode).toBe(0);
    const data = okData<SpecAttachData>(attached.envelope);
    expectSpecMetadata(data, taskId, "stdin-actor");
    expect(data.spec.bytes).toBe(markdown.length);
    expect(await readFile(join(repo, ".tasque", "specs", taskId, "spec.md"), "utf8")).toBe(
      markdown,
    );
  });

  it("returns validation error when multiple sources are provided", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const taskId = await createTask(repo, "Spec conflict task");
    const sourcePath = join(repo, "conflict-spec.md");
    await writeFile(sourcePath, "# conflict", "utf8");

    const result = await runJson(repo, [
      "spec",
      "attach",
      taskId,
      "--text",
      "inline",
      "--file",
      sourcePath,
    ]);

    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("returns validation error when no source is provided", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const taskId = await createTask(repo, "Spec missing source task");

    const result = await runJson(repo, ["spec", "attach", taskId]);

    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("returns JSON envelope with spec metadata", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const taskId = await createTask(repo, "Spec metadata task");

    const result = await runJson(repo, ["spec", "attach", taskId, "--text", "# metadata"]);
    expect(result.exitCode).toBe(0);
    expect(result.envelope.command).toBe("tsq spec attach");

    const data = okData<SpecAttachData>(result.envelope);
    expectSpecMetadata(data, taskId, "test-spec");
    expect(data.spec.spec_path).toBe(`.tasque/specs/${taskId}/spec.md`);
  });
});

describe("cli spec check", () => {
  it("returns success for a valid canonical spec", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const taskId = await createTask(repo, "Spec check success task");
    await runJson(repo, ["spec", "attach", taskId, "--text", VALID_SPEC_MARKDOWN]);

    const checked = await runJson(repo, ["spec", "check", taskId]);

    expect(checked.exitCode).toBe(0);
    expect(checked.envelope.ok).toBe(true);
    expect(checked.envelope.command).toBe("tsq spec check");
    const data = okData<SpecCheckData>(checked.envelope);
    expect(data.task_id).toBe(taskId);
    expect(data.ok).toBe(true);
    expect(data.spec.spec_path).toBe(`.tasque/specs/${taskId}/spec.md`);
    expect(data.spec.attached).toBe(true);
    expect(data.spec.missing_sections.length).toBe(0);
    expect(data.diagnostics.length).toBe(0);
    if (data.spec.expected_fingerprint && data.spec.actual_fingerprint) {
      expect(data.spec.expected_fingerprint).toBe(data.spec.actual_fingerprint);
    }
  });

  it("detects fingerprint drift after canonical spec mutation", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const taskId = await createTask(repo, "Spec check drift task");
    await runJson(repo, ["spec", "attach", taskId, "--text", VALID_SPEC_MARKDOWN]);
    await writeFile(
      join(repo, ".tasque", "specs", taskId, "spec.md"),
      `${VALID_SPEC_MARKDOWN}\n<!-- drift -->\n`,
      "utf8",
    );

    const checked = await runJson(repo, ["spec", "check", taskId]);

    expect(checked.exitCode).toBe(0);
    expect(checked.envelope.command).toBe("tsq spec check");
    const data = okData<SpecCheckData>(checked.envelope);
    expect(data.task_id).toBe(taskId);
    expect(data.ok).toBe(false);
    const diagnosticCodes = data.diagnostics.map((diagnostic) => diagnostic.code);
    expect(diagnosticCodes).toContain("SPEC_FINGERPRINT_DRIFT");
    expect(data.spec.expected_fingerprint).toBeDefined();
    expect(data.spec.actual_fingerprint).toBeDefined();
    expect(data.spec.expected_fingerprint).not.toBe(data.spec.actual_fingerprint);
  });

  it("detects missing required sections", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const taskId = await createTask(repo, "Spec check missing section task");
    const incompleteSpec = `# Thin Spec

## Overview
Only overview exists here.
`;
    await runJson(repo, ["spec", "attach", taskId, "--text", incompleteSpec]);

    const checked = await runJson(repo, ["spec", "check", taskId]);

    expect(checked.exitCode).toBe(0);
    expect(checked.envelope.command).toBe("tsq spec check");
    const data = okData<SpecCheckData>(checked.envelope);
    expect(data.task_id).toBe(taskId);
    expect(data.ok).toBe(false);
    const diagnosticCodes = data.diagnostics.map((diagnostic) => diagnostic.code);
    expect(diagnosticCodes).toContain("SPEC_REQUIRED_SECTIONS_MISSING");
    expect(data.spec.missing_sections.length).toBeGreaterThan(0);
    expect(data.spec.missing_sections).toEqual(
      expect.arrayContaining(["Constraints / Non-goals", "Acceptance criteria"]),
    );
  });
});
