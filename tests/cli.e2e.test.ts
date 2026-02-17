import { afterEach, describe, expect, it } from "bun:test";
import { access, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import type { SkillOperationResult, SkillOperationSummary, SkillTarget } from "../src/skills/types";

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

interface CliResult {
  exitCode: number;
  stdout: string;
  stderr: string;
}

interface InitData {
  files: string[];
  skill_operation?: SkillOperationSummary;
}

const repos: string[] = [];
const repoRoot = resolve(import.meta.dir, "..");
const cliEntry = join(repoRoot, "src", "main.ts");

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-cli-e2e-"));
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

async function runJson(repoDir: string, args: string[]): Promise<JsonResult> {
  const proc = Bun.spawn({
    cmd: ["bun", "run", cliEntry, ...args, "--json"],
    cwd: repoDir,
    env: {
      ...process.env,
      TSQ_ACTOR: "task4-e2e",
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

async function runCli(repoDir: string, args: string[]): Promise<CliResult> {
  const proc = Bun.spawn({
    cmd: ["bun", "run", cliEntry, ...args],
    cwd: repoDir,
    env: {
      ...process.env,
      TSQ_ACTOR: "task4-e2e",
    },
    stdout: "pipe",
    stderr: "pipe",
  });

  const [exitCode, stdout, stderr] = await Promise.all([
    proc.exited,
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
  ]);

  return {
    exitCode,
    stdout,
    stderr,
  };
}

function okData<T>(envelope: JsonEnvelope): T {
  expect(envelope.ok).toBe(true);
  return envelope.data as T;
}

function collectObjects(value: unknown): Record<string, unknown>[] {
  const out: Record<string, unknown>[] = [];
  const visit = (input: unknown): void => {
    if (Array.isArray(input)) {
      for (const item of input) {
        visit(item);
      }
      return;
    }
    if (input && typeof input === "object") {
      const record = input as Record<string, unknown>;
      out.push(record);
      for (const nested of Object.values(record)) {
        visit(nested);
      }
    }
  };
  visit(value);
  return out;
}

function containsTaskRef(value: unknown, taskId: string): boolean {
  if (typeof value === "string") {
    return value === taskId;
  }
  if (Array.isArray(value)) {
    return value.some((entry) => containsTaskRef(entry, taskId));
  }
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    if (record.id === taskId) {
      return true;
    }
    return Object.values(record).some((entry) => containsTaskRef(entry, taskId));
  }
  return false;
}

function mustSkillResult(data: InitData, target: SkillTarget): SkillOperationResult {
  const operation = data.skill_operation;
  expect(operation).toBeDefined();
  const result = operation?.results.find((entry) => entry.target === target);
  expect(result).toBeDefined();
  return result as SkillOperationResult;
}

async function pathExists(path: string): Promise<boolean> {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

describe("cli e2e", () => {
  it("covers init/create/show/list/ready happy path", async () => {
    const repo = await makeRepo();

    const init = await runJson(repo, ["init"]);
    expect(init.exitCode).toBe(0);
    expect(init.envelope.ok).toBe(true);

    const created = await runJson(repo, ["create", "Task 4 happy path"]);
    expect(created.exitCode).toBe(0);
    const createdTask = okData<{ task: { id: string; title: string } }>(created.envelope).task;
    expect(createdTask.title).toBe("Task 4 happy path");

    const shown = await runJson(repo, ["show", createdTask.id]);
    expect(shown.exitCode).toBe(0);
    const shownData = okData<{ task: { id: string } }>(shown.envelope);
    expect(shownData.task.id).toBe(createdTask.id);

    const listed = await runJson(repo, ["list"]);
    expect(listed.exitCode).toBe(0);
    const listedIds = okData<{ tasks: Array<{ id: string }> }>(listed.envelope).tasks.map(
      (task) => task.id,
    );
    expect(listedIds.includes(createdTask.id)).toBe(true);

    const ready = await runJson(repo, ["ready"]);
    expect(ready.exitCode).toBe(0);
    const readyIds = okData<{ tasks: Array<{ id: string }> }>(ready.envelope).tasks.map(
      (task) => task.id,
    );
    expect(readyIds.includes(createdTask.id)).toBe(true);
  });

  it("removes blocked task from ready list after dep add", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const child = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Child task"])).envelope,
    ).task;
    const blocker = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Blocker task"])).envelope,
    ).task;

    const readyBeforeIds = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["ready"])).envelope,
    ).tasks.map((task) => task.id);
    expect(readyBeforeIds.includes(child.id)).toBe(true);

    const depAdd = await runJson(repo, ["dep", "add", child.id, blocker.id]);
    expect(depAdd.exitCode).toBe(0);
    expect(depAdd.envelope.ok).toBe(true);

    const readyAfterIds = okData<{ tasks: Array<{ id: string }> }>(
      (await runJson(repo, ["ready"])).envelope,
    ).tasks.map((task) => task.id);
    expect(readyAfterIds.includes(child.id)).toBe(false);
    expect(readyAfterIds.includes(blocker.id)).toBe(true);
  });

  it("supersede closes source task and sets superseded_by", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const oldTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Old task"])).envelope,
    ).task;
    const replacement = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Replacement task"])).envelope,
    ).task;

    const superseded = await runJson(repo, [
      "supersede",
      oldTask.id,
      "--with",
      replacement.id,
      "--reason",
      "obsolete",
    ]);
    expect(superseded.exitCode).toBe(0);
    const supersededTask = okData<{
      task: { id: string; status: string; superseded_by?: string; closed_at?: string };
    }>(superseded.envelope).task;
    expect(supersededTask.id).toBe(oldTask.id);
    expect(supersededTask.status).toBe("closed");
    expect(supersededTask.superseded_by).toBe(replacement.id);
    expect(typeof supersededTask.closed_at).toBe("string");

    const shown = okData<{
      task: { status: string; superseded_by?: string };
    }>((await runJson(repo, ["show", oldTask.id])).envelope);
    expect(shown.task.status).toBe("closed");
    expect(shown.task.superseded_by).toBe(replacement.id);
  });

  it("returns ambiguous error for non-unique partial ID", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    await runJson(repo, ["create", "Alpha"]);
    await runJson(repo, ["create", "Beta"]);

    const ambiguous = await runJson(repo, ["show", "tsq-"]);
    expect(ambiguous.exitCode).toBe(1);
    expect(ambiguous.envelope.ok).toBe(false);
    expect(ambiguous.envelope.error?.code).toBe("TASK_ID_AMBIGUOUS");
    expect(ambiguous.envelope.error?.details).toBeObject();

    const details = ambiguous.envelope.error?.details as { candidates?: string[] } | undefined;
    expect(Array.isArray(details?.candidates)).toBe(true);
    expect((details?.candidates?.length ?? 0) >= 2).toBe(true);
  });

  it("prints tree list with parent child hierarchy and dependency context", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const parent = okData<{ task: { id: string; title: string } }>(
      (await runJson(repo, ["create", "Tree parent"])).envelope,
    ).task;
    const child = okData<{ task: { id: string; title: string } }>(
      (await runJson(repo, ["create", "Tree child", "--parent", parent.id])).envelope,
    ).task;
    const blocker = okData<{ task: { id: string; title: string } }>(
      (await runJson(repo, ["create", "Tree blocker"])).envelope,
    ).task;
    await runJson(repo, ["dep", "add", child.id, blocker.id]);

    const listed = await runCli(repo, ["list", "--tree"]);
    expect(listed.exitCode).toBe(0);
    expect(listed.stderr.trim()).toBe("");

    const lines = listed.stdout
      .split(/\r?\n/u)
      .map((line) => line.trimEnd())
      .filter((line) => line.length > 0);

    const parentLine = lines.find((line) => line.includes(parent.id));
    const childLine = lines.find((line) => line.includes(child.id));
    expect(parentLine).toBeDefined();
    expect(childLine).toBeDefined();
    expect(lines.findIndex((line) => line.includes(parent.id))).toBeLessThan(
      lines.findIndex((line) => line.includes(child.id)),
    );
    expect(parentLine?.includes(parent.title)).toBe(true);
    expect(childLine?.includes(child.title)).toBe(true);

    const childIndent = childLine ? childLine.indexOf(child.id) : -1;
    expect(childIndent > 0 || /[├└│]/u.test(childLine ?? "")).toBe(true);

    const blockerMentions = lines.filter(
      (line) => line.includes(blocker.id) || line.includes(blocker.title),
    );
    expect(blockerMentions.length).toBeGreaterThanOrEqual(2);
    expect(
      blockerMentions.some(
        (line) =>
          line.includes(child.id) ||
          line.includes(child.title) ||
          /block|depend|dep|wait/u.test(line.toLowerCase()),
      ),
    ).toBe(true);
  });

  it("returns tree-aware json envelope data with hierarchy and dependency fields", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const parent = okData<{ task: { id: string; title: string } }>(
      (await runJson(repo, ["create", "JSON tree parent"])).envelope,
    ).task;
    const child = okData<{ task: { id: string; title: string } }>(
      (await runJson(repo, ["create", "JSON tree child", "--parent", parent.id])).envelope,
    ).task;
    const blocker = okData<{ task: { id: string; title: string } }>(
      (await runJson(repo, ["create", "JSON tree blocker"])).envelope,
    ).task;
    await runJson(repo, ["dep", "add", child.id, blocker.id]);

    const listed = await runJson(repo, ["list", "--tree"]);
    expect(listed.exitCode).toBe(0);
    expect(listed.envelope.command).toBe("tsq list");

    const data = okData<Record<string, unknown>>(listed.envelope);
    expect(data).toBeObject();

    const objects = collectObjects(data);
    const parentNode = objects.find((entry) => entry.id === parent.id);
    const childNode = objects.find((entry) => entry.id === child.id);
    const blockerNode = objects.find((entry) => entry.id === blocker.id);
    expect(parentNode).toBeDefined();
    expect(childNode).toBeDefined();
    expect(blockerNode).toBeDefined();

    expect(typeof parentNode?.title).toBe("string");
    expect(typeof childNode?.title).toBe("string");
    expect(typeof parentNode?.status).toBe("string");
    expect(typeof childNode?.status).toBe("string");

    const hasHierarchyContext = Boolean(
      childNode?.parent_id === parent.id ||
        childNode?.parent === parent.id ||
        containsTaskRef(parentNode?.children, child.id),
    );
    expect(hasHierarchyContext).toBe(true);

    const hasDependencyOnChildNode = childNode
      ? Object.entries(childNode).some(([key, value]) => {
          if (!/block|depend|dep|wait/u.test(key.toLowerCase())) {
            return false;
          }
          return containsTaskRef(value, blocker.id);
        })
      : false;
    const hasDependencyContextElsewhere = objects.some((entry) => {
      const depKeys = Object.keys(entry).filter((key) =>
        /block|depend|dep|wait/u.test(key.toLowerCase()),
      );
      if (depKeys.length === 0) {
        return false;
      }
      return containsTaskRef(entry, child.id) && containsTaskRef(entry, blocker.id);
    });
    expect(hasDependencyOnChildNode || hasDependencyContextElsewhere).toBe(true);
  });

  it("installs skill files for all targets using override directories", async () => {
    const repo = await makeRepo();
    const claudeDir = join(repo, "skills-claude");
    const codexDir = join(repo, "skills-codex");
    const copilotDir = join(repo, "skills-copilot");
    const opencodeDir = join(repo, "skills-opencode");

    const init = await runJson(repo, [
      "init",
      "--install-skill",
      "--skill-targets",
      "all",
      "--skill-dir-claude",
      claudeDir,
      "--skill-dir-codex",
      codexDir,
      "--skill-dir-copilot",
      copilotDir,
      "--skill-dir-opencode",
      opencodeDir,
    ]);
    expect(init.exitCode).toBe(0);
    const data = okData<InitData>(init.envelope);
    expect(data.skill_operation?.action).toBe("install");
    expect(data.skill_operation?.results.length).toBe(4);

    const targets: Array<[SkillTarget, string]> = [
      ["claude", claudeDir],
      ["codex", codexDir],
      ["copilot", copilotDir],
      ["opencode", opencodeDir],
    ];
    for (const [target, rootDir] of targets) {
      const result = mustSkillResult(data, target);
      expect(result.status).toBe("installed");
      const skillPath = join(rootDir, "tasque");
      expect(await pathExists(join(skillPath, "SKILL.md"))).toBe(true);
      expect(await pathExists(join(skillPath, "README.md"))).toBe(true);
    }
  });

  it("skips install when unmanaged skill exists and force is not set", async () => {
    const repo = await makeRepo();
    const codexDir = join(repo, "skills-codex");
    const skillPath = join(codexDir, "tasque");

    const firstInstall = await runJson(repo, [
      "init",
      "--install-skill",
      "--skill-targets",
      "codex",
      "--skill-dir-codex",
      codexDir,
    ]);
    expect(firstInstall.exitCode).toBe(0);

    await writeFile(join(skillPath, "SKILL.md"), "# unmanaged\n", "utf8");
    await writeFile(join(skillPath, "README.md"), "manual content\n", "utf8");

    const secondInstall = await runJson(repo, [
      "init",
      "--install-skill",
      "--skill-targets",
      "codex",
      "--skill-dir-codex",
      codexDir,
    ]);
    expect(secondInstall.exitCode).toBe(0);
    const data = okData<InitData>(secondInstall.envelope);
    const result = mustSkillResult(data, "codex");
    expect(result.status).toBe("skipped");
    expect(await readFile(join(skillPath, "SKILL.md"), "utf8")).toBe("# unmanaged\n");
    expect(await readFile(join(skillPath, "README.md"), "utf8")).toBe("manual content\n");
  });

  it("uninstalls managed skill directory", async () => {
    const repo = await makeRepo();
    const codexDir = join(repo, "skills-codex");
    const skillPath = join(codexDir, "tasque");

    await runJson(repo, [
      "init",
      "--install-skill",
      "--skill-targets",
      "codex",
      "--skill-dir-codex",
      codexDir,
    ]);
    expect(await pathExists(skillPath)).toBe(true);

    const uninstall = await runJson(repo, [
      "init",
      "--uninstall-skill",
      "--skill-targets",
      "codex",
      "--skill-dir-codex",
      codexDir,
    ]);
    expect(uninstall.exitCode).toBe(0);
    const data = okData<InitData>(uninstall.envelope);
    const result = data.skill_operation?.results[0];
    expect(result).toBeDefined();
    if (!result) {
      throw new Error("missing uninstall result");
    }
    expect(result.status).toBe("removed");
    expect(await pathExists(skillPath)).toBe(false);
  });

  it("claim sets status to in_progress for open task", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Claim target"])).envelope,
    ).task;

    const claimed = await runJson(repo, ["update", created.id, "--claim"]);
    expect(claimed.exitCode).toBe(0);

    const shown = okData<{ task: { id: string; status: string; assignee?: string } }>(
      (await runJson(repo, ["show", created.id])).envelope,
    );
    expect(shown.task.status).toBe("in_progress");
    expect(typeof shown.task.assignee).toBe("string");
  });

  it("claim preserves in_progress status when already in_progress", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Already started"])).envelope,
    ).task;

    await runJson(repo, ["update", created.id, "--status", "in_progress"]);

    const claimed = await runJson(repo, ["update", created.id, "--claim", "--assignee", "bob"]);
    expect(claimed.exitCode).toBe(0);

    const shown = okData<{ task: { id: string; status: string; assignee?: string } }>(
      (await runJson(repo, ["show", created.id])).envelope,
    );
    expect(shown.task.status).toBe("in_progress");
    expect(shown.task.assignee).toBe("bob");
  });

  it("returns validation error when install and uninstall flags are combined", async () => {
    const repo = await makeRepo();
    const combined = await runJson(repo, ["init", "--install-skill", "--uninstall-skill"]);
    expect(combined.exitCode).toBe(1);
    expect(combined.envelope.ok).toBe(false);
    expect(combined.envelope.error?.code).toBe("VALIDATION_ERROR");
  });
});
