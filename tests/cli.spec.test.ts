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

const repos: string[] = [];
const repoRoot = resolve(import.meta.dir, "..");
const cliEntry = join(repoRoot, "src", "main.ts");

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
