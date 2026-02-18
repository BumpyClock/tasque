/**
 * Storage IO adapter — consolidates all file-system I/O used by TasqueService.
 *
 * Re-exports projection loading/persistence from `./state` and store modules,
 * plus houses spec-file, gitignore, and event-file init helpers that were
 * previously inlined in service.ts.
 */

import { createHash } from "node:crypto";
import { mkdir, open, readFile, rename, unlink } from "node:fs/promises";
import { dirname, isAbsolute, join } from "node:path";
import { TsqError } from "../errors";
import { getPaths, taskSpecFile, taskSpecRelativePath } from "../store/paths";
import type { Task } from "../types";

// ── Re-exports from existing modules ────────────────────────────────────────
export { loadProjectedState, persistProjection } from "./state";
export type { LoadedState } from "./state";
export { appendEvents, readEvents } from "../store/events";
export { withWriteLock } from "../store/lock";
export { writeDefaultConfig, readConfig } from "../store/config";
export { writeSnapshot, loadLatestSnapshot } from "../store/snapshots";
export { writeStateCache, readStateCache } from "../store/state";
export { getPaths, taskSpecFile, taskSpecRelativePath } from "../store/paths";

// ── Spec-related types ──────────────────────────────────────────────────────

/** Describes where spec content comes from during attach. */
export type SpecAttachSource =
  | { type: "file"; path: string }
  | { type: "stdin" }
  | { type: "text"; content: string };

/** Diagnostic code emitted by spec validation. */
export type SpecCheckDiagnosticCode =
  | "SPEC_NOT_ATTACHED"
  | "SPEC_METADATA_INVALID"
  | "SPEC_FILE_MISSING"
  | "SPEC_FINGERPRINT_DRIFT"
  | "SPEC_REQUIRED_SECTIONS_MISSING";

/** Single diagnostic from spec validation. */
export interface SpecCheckDiagnostic {
  code: SpecCheckDiagnosticCode;
  message: string;
  details?: Record<string, unknown>;
}

/** Full result of evaluating a task spec. */
export interface SpecCheckResult {
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
  diagnostics: SpecCheckDiagnostic[];
}

/** Result of writing a spec file atomically. */
export interface SpecWriteResult {
  specPath: string;
  content: string;
}

// ── Required spec sections ──────────────────────────────────────────────────

const REQUIRED_SPEC_SECTIONS = [
  {
    label: "Overview",
    aliases: ["Overview"],
  },
  {
    label: "Constraints / Non-goals",
    aliases: ["Constraints / Non-goals", "Constraints", "Non-goals"],
  },
  {
    label: "Interfaces (CLI/API)",
    aliases: ["Interfaces (CLI/API)", "Interfaces"],
  },
  {
    label: "Data model / schema changes",
    aliases: ["Data model / schema changes", "Data model", "Schema changes"],
  },
  {
    label: "Acceptance criteria",
    aliases: ["Acceptance criteria"],
  },
  {
    label: "Test plan",
    aliases: ["Test plan"],
  },
] as const;

// ── Event file initialization ───────────────────────────────────────────────

export async function ensureEventsFile(repoRoot: string): Promise<void> {
  const paths = getPaths(repoRoot);
  await mkdir(paths.tasqueDir, { recursive: true });
  try {
    await readFile(paths.eventsFile, "utf8");
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code !== "ENOENT") {
      throw new TsqError("IO_ERROR", "failed reading events file", 2, error);
    }
    const handle = await open(paths.eventsFile, "a");
    await handle.close();
  }
}

// ── Gitignore management ────────────────────────────────────────────────────

export async function ensureTasqueGitignore(repoRoot: string): Promise<void> {
  const target = join(getPaths(repoRoot).tasqueDir, ".gitignore");
  const desired = ["state.json", "state.json.tmp*", ".lock", "snapshots/", "snapshots/*.tmp"];
  try {
    await Bun.write(target, `${desired.join("\n")}\n`);
  } catch (error) {
    throw new TsqError("IO_ERROR", "failed writing .tasque/.gitignore", 2, error);
  }
}

// ── Spec file I/O ───────────────────────────────────────────────────────────

export async function writeTaskSpecAtomic(
  repoRoot: string,
  taskId: string,
  content: string,
): Promise<SpecWriteResult> {
  const specFile = taskSpecFile(repoRoot, taskId);
  const specPath = taskSpecRelativePath(taskId);
  await mkdir(dirname(specFile), { recursive: true });
  const temp = `${specFile}.tmp-${process.pid}-${Date.now()}`;

  try {
    const handle = await open(temp, "w");
    try {
      await handle.writeFile(content, "utf8");
      await handle.sync();
    } finally {
      await handle.close();
    }
    await rename(temp, specFile);
    return {
      specPath,
      content: await readFile(specFile, "utf8"),
    };
  } catch (error) {
    try {
      await unlink(temp);
    } catch {
      // best-effort cleanup
    }
    throw new TsqError("IO_ERROR", "failed writing attached spec", 2, error);
  }
}

export async function evaluateTaskSpec(
  repoRoot: string,
  taskId: string,
  task: Task,
): Promise<SpecCheckResult> {
  const specPath = normalizeOptionalInput(task.spec_path);
  const expectedFingerprint = normalizeOptionalInput(task.spec_fingerprint);
  const requiredSections = REQUIRED_SPEC_SECTIONS.map((section) => section.label);
  const diagnostics: SpecCheckDiagnostic[] = [];
  let presentSections: string[] = [];
  let missingSections = [...requiredSections];
  let actualFingerprint: string | undefined;
  let bytes: number | undefined;
  let content: string | undefined;

  if (!specPath && !expectedFingerprint) {
    diagnostics.push({
      code: "SPEC_NOT_ATTACHED",
      message: "task does not have an attached spec",
    });
  } else if (!specPath || !expectedFingerprint) {
    diagnostics.push({
      code: "SPEC_METADATA_INVALID",
      message: "task spec metadata is incomplete",
      details: {
        has_spec_path: specPath !== undefined,
        has_spec_fingerprint: expectedFingerprint !== undefined,
      },
    });
  }

  if (specPath) {
    try {
      content = await readFile(resolveSpecPath(repoRoot, specPath), "utf8");
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (code === "ENOENT") {
        diagnostics.push({
          code: "SPEC_FILE_MISSING",
          message: "attached spec file not found",
          details: {
            spec_path: specPath,
          },
        });
      } else {
        throw new TsqError("IO_ERROR", `failed reading attached spec file: ${specPath}`, 2, error);
      }
    }
  }

  if (content !== undefined) {
    bytes = Buffer.byteLength(content, "utf8");
    actualFingerprint = sha256(content);
    if (expectedFingerprint && actualFingerprint !== expectedFingerprint) {
      diagnostics.push({
        code: "SPEC_FINGERPRINT_DRIFT",
        message: "spec fingerprint drift detected",
        details: {
          expected_fingerprint: expectedFingerprint,
          actual_fingerprint: actualFingerprint,
        },
      });
    }

    presentSections = extractMarkdownHeadings(content);
    const presentNormalized = new Set(
      presentSections.map((section) => normalizeMarkdownHeading(section)),
    );
    missingSections = REQUIRED_SPEC_SECTIONS.filter((required) => {
      return !required.aliases.some((alias) =>
        presentNormalized.has(normalizeMarkdownHeading(alias)),
      );
    }).map((required) => required.label);
    if (missingSections.length > 0) {
      diagnostics.push({
        code: "SPEC_REQUIRED_SECTIONS_MISSING",
        message: "spec is missing required markdown sections",
        details: {
          missing_sections: missingSections,
        },
      });
    }
  }

  return {
    task_id: taskId,
    ok: diagnostics.length === 0,
    spec: {
      attached: Boolean(specPath && expectedFingerprint),
      spec_path: specPath,
      expected_fingerprint: expectedFingerprint,
      actual_fingerprint: actualFingerprint,
      ...(bytes === undefined ? {} : { bytes }),
      required_sections: requiredSections,
      present_sections: presentSections,
      missing_sections: missingSections,
    },
    diagnostics,
  };
}

// ── Spec attach source resolution ───────────────────────────────────────────

export interface SpecAttachInput {
  file?: string;
  source?: string;
  text?: string;
  stdin?: boolean;
}

export function resolveSpecAttachSource(input: SpecAttachInput): SpecAttachSource {
  const file = normalizeOptionalInput(input.file);
  const positional = normalizeOptionalInput(input.source);
  const hasStdin = input.stdin === true;
  const hasText = input.text !== undefined;

  const sourcesProvided = [file !== undefined, positional !== undefined, hasStdin, hasText].filter(
    (value) => value,
  ).length;
  if (sourcesProvided !== 1) {
    throw new TsqError(
      "VALIDATION_ERROR",
      "exactly one source is required: --file, --stdin, --text, or positional source path",
      1,
    );
  }

  if (hasText) {
    return { type: "text", content: input.text ?? "" };
  }
  if (hasStdin) {
    return { type: "stdin" };
  }
  return { type: "file", path: file ?? positional ?? "" };
}

export async function readSpecAttachContent(source: SpecAttachSource): Promise<string> {
  if (source.type === "text") {
    return source.content;
  }
  if (source.type === "stdin") {
    return readStdinContent();
  }

  try {
    return await readFile(source.path, "utf8");
  } catch (error) {
    throw new TsqError("IO_ERROR", `failed reading spec source file: ${source.path}`, 2, error);
  }
}

// ── Hashing ─────────────────────────────────────────────────────────────────

export function sha256(content: string): string {
  return createHash("sha256").update(content, "utf8").digest("hex");
}

// ── Internal helpers ────────────────────────────────────────────────────────

const STDIN_TIMEOUT_MS = 30_000;

async function readStdinContent(): Promise<string> {
  process.stdin.setEncoding("utf8");
  let content = "";

  const readAll = async (): Promise<string> => {
    for await (const chunk of process.stdin) {
      content += chunk;
    }
    return content;
  };

  const timeout = new Promise<never>((_resolve, reject) => {
    const timer = setTimeout(() => {
      reject(
        new TsqError(
          "VALIDATION_ERROR",
          `stdin read timed out after ${STDIN_TIMEOUT_MS / 1000} seconds`,
          1,
        ),
      );
    }, STDIN_TIMEOUT_MS);
    if (typeof timer === "object" && "unref" in timer) {
      timer.unref();
    }
  });

  const result = await Promise.race([readAll(), timeout]);

  if (result.trim().length === 0) {
    throw new TsqError("VALIDATION_ERROR", "stdin content must not be empty", 1);
  }

  return result;
}

function resolveSpecPath(repoRoot: string, specPath: string): string {
  if (isAbsolute(specPath)) {
    return specPath;
  }
  return join(repoRoot, specPath);
}

export function normalizeOptionalInput(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function extractMarkdownHeadings(content: string): string[] {
  const headings: string[] = [];
  const seen = new Set<string>();
  for (const match of content.matchAll(/^#{1,6}[ \t]+(.+?)\s*$/gmu)) {
    const heading = (match[1] ?? "").replace(/[ \t]+#+\s*$/u, "").trim();
    if (heading.length === 0) {
      continue;
    }
    const key = heading.toLowerCase();
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    headings.push(heading);
  }
  return headings;
}

function normalizeMarkdownHeading(heading: string): string {
  return heading
    .trim()
    .replace(/\s+/gu, " ")
    .replace(/\s*:\s*$/u, "")
    .toLowerCase();
}
