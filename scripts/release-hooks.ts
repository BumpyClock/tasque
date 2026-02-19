import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { SCHEMA_VERSION } from "../src/types";

type TaskKind = "task" | "feature" | "epic";

interface TsqEnvelope {
  ok?: boolean;
  error?: { code?: string; message?: string };
  data?: {
    tree?: TsqTreeNode[];
  };
}

interface TsqTreeNode {
  task: TsqTask;
  children: TsqTreeNode[];
}

interface TsqTask {
  id: string;
  title: string;
  kind: TaskKind;
  status: string;
  priority: number;
  assignee?: string;
  parent_id?: string;
  closed_at?: string;
}

interface CommandResult {
  code: number;
  stdout: string;
  stderr: string;
}

type CommandRunner = (cmd: string[], cwd: string) => Promise<CommandResult>;

export interface ReleaseBaseline {
  tag: string;
  ts: string;
}

export interface ReleaseTask {
  id: string;
  title: string;
  kind: TaskKind;
  priority: number;
  closed_at: string;
  assignee?: string;
  parent_id?: string;
}

export interface ReleaseNotes {
  schema_version: typeof SCHEMA_VERSION;
  version: string;
  generated_at: string;
  baseline: ReleaseBaseline | null;
  counts: {
    total: number;
    task: number;
    feature: number;
    epic: number;
    p0: number;
    p1: number;
    p2: number;
    p3: number;
  };
  items: ReleaseTask[];
}

export interface GenerateReleaseNotesOptions {
  repoRoot: string;
  releaseDir: string;
  version: string;
  tsqBin?: string;
  generatedAt?: string;
  runCommand?: CommandRunner;
}

export interface GenerateReleaseNotesResult {
  baseline: ReleaseBaseline | null;
  notes: ReleaseNotes;
  markdownPath: string;
  jsonPath: string;
}

const NOTES_MARKDOWN = "RELEASE_NOTES.md";
const NOTES_JSON = "RELEASE_NOTES.json";

export async function generateReleaseNotesArtifacts(
  options: GenerateReleaseNotesOptions,
): Promise<GenerateReleaseNotesResult> {
  const runCommand = options.runCommand ?? runShellCommand;
  const baseline = await resolveLatestTagBaseline(options.repoRoot, runCommand);
  const tree = await readTaskTree(options.repoRoot, options.tsqBin, runCommand);
  const tasks = selectReleaseTasks(flattenTree(tree), baseline?.ts);
  const notes = buildReleaseNotes(
    options.version,
    baseline,
    tasks,
    options.generatedAt ?? new Date().toISOString(),
  );

  await mkdir(options.releaseDir, { recursive: true });
  const markdownPath = join(options.releaseDir, NOTES_MARKDOWN);
  const jsonPath = join(options.releaseDir, NOTES_JSON);
  await writeFile(markdownPath, renderReleaseNotesMarkdown(notes), "utf8");
  await writeFile(jsonPath, renderReleaseNotesJson(notes), "utf8");

  return { baseline, notes, markdownPath, jsonPath };
}

export async function resolveLatestTagBaseline(
  repoRoot: string,
  runCommand: CommandRunner = runShellCommand,
): Promise<ReleaseBaseline | null> {
  const result = await runCommand(
    [
      "git",
      "for-each-ref",
      "refs/tags",
      "--sort=-creatordate",
      "--format=%(refname:short)%09%(creatordate:iso-strict)",
      "--count=1",
    ],
    repoRoot,
  );
  if (result.code !== 0) {
    throw new Error(
      `failed reading git tags: ${result.stderr.trim() || `exit code ${result.code}`}`,
    );
  }
  const line = result.stdout.trim();
  if (line.length === 0) {
    return null;
  }

  const [tag, ts] = line.split(/\t/u);
  if (!tag || !ts || Number.isNaN(Date.parse(ts))) {
    throw new Error(`invalid latest tag baseline output: ${line}`);
  }
  return { tag, ts };
}

export function selectReleaseTasks(tasks: TsqTask[], baselineTs?: string): ReleaseTask[] {
  const baselineMillis = baselineTs ? Date.parse(baselineTs) : Number.NaN;
  return tasks
    .filter((task) => task.status === "closed")
    .filter(
      (task) => typeof task.closed_at === "string" && !Number.isNaN(Date.parse(task.closed_at)),
    )
    .filter((task) => {
      if (!baselineTs || Number.isNaN(baselineMillis)) {
        return true;
      }
      return Date.parse(task.closed_at as string) > baselineMillis;
    })
    .map((task) => ({
      id: task.id,
      title: task.title,
      kind: task.kind,
      priority: task.priority,
      closed_at: task.closed_at as string,
      assignee: task.assignee,
      parent_id: task.parent_id,
    }))
    .sort((a, b) => {
      const timeDiff = Date.parse(b.closed_at) - Date.parse(a.closed_at);
      if (timeDiff !== 0) {
        return timeDiff;
      }
      return a.id.localeCompare(b.id);
    });
}

export function buildReleaseNotes(
  version: string,
  baseline: ReleaseBaseline | null,
  tasks: ReleaseTask[],
  generatedAt: string,
): ReleaseNotes {
  const counts = {
    total: tasks.length,
    task: 0,
    feature: 0,
    epic: 0,
    p0: 0,
    p1: 0,
    p2: 0,
    p3: 0,
  };

  for (const task of tasks) {
    counts[task.kind] += 1;
    if (task.priority === 0) counts.p0 += 1;
    else if (task.priority === 1) counts.p1 += 1;
    else if (task.priority === 2) counts.p2 += 1;
    else counts.p3 += 1;
  }

  return {
    schema_version: SCHEMA_VERSION,
    version,
    generated_at: generatedAt,
    baseline,
    counts,
    items: tasks,
  };
}

export function renderReleaseNotesJson(notes: ReleaseNotes): string {
  return `${JSON.stringify(notes, null, 2)}\n`;
}

export function renderReleaseNotesMarkdown(notes: ReleaseNotes): string {
  const lines: string[] = [];
  lines.push(`# Release Notes v${notes.version}`);
  lines.push("");
  lines.push(`generated_at: ${notes.generated_at}`);
  if (notes.baseline) {
    lines.push(`baseline: ${notes.baseline.tag} (${notes.baseline.ts})`);
  } else {
    lines.push("baseline: none (no git tag found)");
  }
  lines.push("");
  lines.push("## Summary");
  lines.push(`- total: ${notes.counts.total}`);
  lines.push(`- epics: ${notes.counts.epic}`);
  lines.push(`- features: ${notes.counts.feature}`);
  lines.push(`- tasks: ${notes.counts.task}`);
  lines.push(
    `- priorities: p0=${notes.counts.p0} p1=${notes.counts.p1} p2=${notes.counts.p2} p3=${notes.counts.p3}`,
  );
  lines.push("");

  const kinds: Array<{ kind: TaskKind; title: string }> = [
    { kind: "epic", title: "Epics" },
    { kind: "feature", title: "Features" },
    { kind: "task", title: "Tasks" },
  ];

  for (const section of kinds) {
    lines.push(`## ${section.title}`);
    const sectionTasks = notes.items.filter((task) => task.kind === section.kind);
    if (sectionTasks.length === 0) {
      lines.push("- none");
      lines.push("");
      continue;
    }
    for (const task of sectionTasks) {
      const assignee = task.assignee ? ` @${task.assignee}` : "";
      lines.push(
        `- [${task.id}] ${task.title} (p${task.priority}, closed ${task.closed_at}${assignee})`,
      );
    }
    lines.push("");
  }

  return `${lines.join("\n")}\n`;
}

export function flattenTree(tree: TsqTreeNode[]): TsqTask[] {
  const tasks: TsqTask[] = [];
  const visit = (node: TsqTreeNode): void => {
    tasks.push(node.task);
    for (const child of node.children ?? []) {
      visit(child);
    }
  };
  for (const node of tree) {
    visit(node);
  }
  return tasks;
}

async function readTaskTree(
  repoRoot: string,
  tsqBin: string | undefined,
  runCommand: CommandRunner,
): Promise<TsqTreeNode[]> {
  const bin = tsqBin ?? process.env.TSQ_BIN ?? (process.platform === "win32" ? "tsq.exe" : "tsq");
  const result = await runCommand([bin, "list", "--tree", "--full", "--json"], repoRoot);
  if (result.code !== 0) {
    throw new Error(
      `failed running tsq list: ${result.stderr.trim() || `exit code ${result.code}`}`,
    );
  }

  let parsed: TsqEnvelope;
  try {
    parsed = JSON.parse(result.stdout) as TsqEnvelope;
  } catch (error) {
    throw new Error(`failed parsing tsq json output: ${(error as Error).message}`);
  }

  if (!parsed.ok) {
    const code = parsed.error?.code ?? "UNKNOWN";
    const message = parsed.error?.message ?? "unknown error";
    throw new Error(`tsq list --json failed: ${code} ${message}`);
  }
  const tree = parsed.data?.tree;
  if (!Array.isArray(tree)) {
    throw new Error("tsq json output missing data.tree");
  }
  return tree;
}

async function runShellCommand(cmd: string[], cwd: string): Promise<CommandResult> {
  const proc = Bun.spawn({
    cmd,
    cwd,
    stdout: "pipe",
    stderr: "pipe",
  });
  const [code, stdout, stderr] = await Promise.all([
    proc.exited,
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
  ]);
  return { code, stdout, stderr };
}
