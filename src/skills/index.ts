import { copyFile, mkdir, readFile, readdir, rm, stat } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { TsqError } from "../errors";
import { MANAGED_MARKER } from "./managed";
import type {
  SkillOperationOptions,
  SkillOperationResult,
  SkillOperationSummary,
  SkillTarget,
} from "./types";

export async function applySkillOperation(
  options: SkillOperationOptions,
): Promise<SkillOperationSummary> {
  const targetDirectories = resolveTargetDirectories(options);
  const skillSourceDirectory =
    options.action === "install"
      ? await resolveManagedSkillSourceDirectory(options.skillName, options.sourceRootDir)
      : undefined;
  const results: SkillOperationResult[] = [];

  for (const target of options.targets) {
    const targetDirectory = targetDirectories[target];
    const skillDirectory = join(targetDirectory, options.skillName);

    if (options.action === "install") {
      if (!skillSourceDirectory) {
        throw new TsqError("INTERNAL_ERROR", "missing managed skill source directory", 2);
      }
      results.push(
        await installSkill({
          force: options.force,
          skillName: options.skillName,
          skillSourceDirectory,
          skillDirectory,
          target,
        }),
      );
      continue;
    }

    results.push(
      await uninstallSkill({
        force: options.force,
        skillDirectory,
        target,
      }),
    );
  }

  return {
    action: options.action,
    skill_name: options.skillName,
    results,
  };
}

type PathKind = "missing" | "file" | "directory";

interface InstallContext {
  force: boolean;
  skillName: string;
  skillSourceDirectory: string;
  skillDirectory: string;
  target: SkillTarget;
}

interface UninstallContext {
  force: boolean;
  skillDirectory: string;
  target: SkillTarget;
}

function resolveTargetDirectories(options: SkillOperationOptions): Record<SkillTarget, string> {
  const defaultHome = homedir();
  const resolvedHome = normalizeDirectory(options.homeDir ?? defaultHome, defaultHome);
  const rawCodexHome = options.codexHome ?? process.env.CODEX_HOME ?? join(resolvedHome, ".codex");
  const resolvedCodexHome = normalizeDirectory(rawCodexHome, resolvedHome);

  const defaults: Record<SkillTarget, string> = {
    claude: join(resolvedHome, ".claude", "skills"),
    codex: join(resolvedCodexHome, "skills"),
    copilot: join(resolvedHome, ".copilot", "skills"),
    opencode: join(resolvedHome, ".opencode", "skills"),
  };

  return {
    claude: normalizeDirectory(options.targetDirOverrides?.claude ?? defaults.claude, resolvedHome),
    codex: normalizeDirectory(options.targetDirOverrides?.codex ?? defaults.codex, resolvedHome),
    copilot: normalizeDirectory(
      options.targetDirOverrides?.copilot ?? defaults.copilot,
      resolvedHome,
    ),
    opencode: normalizeDirectory(
      options.targetDirOverrides?.opencode ?? defaults.opencode,
      resolvedHome,
    ),
  };
}

function normalizeDirectory(directory: string, home: string): string {
  const expandedDirectory = expandHome(directory, home);
  if (isAbsolute(expandedDirectory)) {
    return expandedDirectory;
  }
  return resolve(expandedDirectory);
}

function expandHome(directory: string, home: string): string {
  if (directory === "~") {
    return home;
  }
  if (directory.startsWith("~/") || directory.startsWith("~\\")) {
    return join(home, directory.slice(2));
  }
  return directory;
}

async function installSkill(context: InstallContext): Promise<SkillOperationResult> {
  const pathKind = await inspectPath(context.skillDirectory);
  if (pathKind === "missing") {
    await copyManagedSkillDirectory(context.skillSourceDirectory, context.skillDirectory);
    return {
      target: context.target,
      path: context.skillDirectory,
      status: "installed",
      message: "installed new managed skill",
    };
  }

  if (pathKind === "file") {
    if (!context.force) {
      return {
        target: context.target,
        path: context.skillDirectory,
        status: "skipped",
        message: "path exists as a non-directory and force is disabled",
      };
    }
    await rm(context.skillDirectory, { force: true });
    await copyManagedSkillDirectory(context.skillSourceDirectory, context.skillDirectory);
    return {
      target: context.target,
      path: context.skillDirectory,
      status: "updated",
      message: "replaced non-directory path with managed skill due to force",
    };
  }

  const managed = await isManagedSkill(context.skillDirectory);
  if (!managed && !context.force) {
    return {
      target: context.target,
      path: context.skillDirectory,
      status: "skipped",
      message: "existing skill is not managed and force is disabled",
    };
  }

  await rm(context.skillDirectory, { recursive: true, force: true });
  await copyManagedSkillDirectory(context.skillSourceDirectory, context.skillDirectory);
  return {
    target: context.target,
    path: context.skillDirectory,
    status: "updated",
    message: managed ? "updated managed skill" : "overwrote non-managed skill due to force",
  };
}

async function uninstallSkill(context: UninstallContext): Promise<SkillOperationResult> {
  const pathKind = await inspectPath(context.skillDirectory);
  if (pathKind === "missing") {
    return {
      target: context.target,
      path: context.skillDirectory,
      status: "not_found",
      message: "skill directory not found",
    };
  }

  if (pathKind === "file") {
    if (!context.force) {
      return {
        target: context.target,
        path: context.skillDirectory,
        status: "skipped",
        message: "path exists as a non-directory and force is disabled",
      };
    }
    await rm(context.skillDirectory, { force: true });
    return {
      target: context.target,
      path: context.skillDirectory,
      status: "removed",
      message: "removed non-directory path due to force",
    };
  }

  const managed = await isManagedSkill(context.skillDirectory);
  if (!managed && !context.force) {
    return {
      target: context.target,
      path: context.skillDirectory,
      status: "skipped",
      message: "existing skill is not managed and force is disabled",
    };
  }

  await rm(context.skillDirectory, { recursive: true, force: true });
  return {
    target: context.target,
    path: context.skillDirectory,
    status: "removed",
    message: managed ? "removed managed skill" : "removed non-managed skill due to force",
  };
}

async function copyManagedSkillDirectory(
  sourceDirectory: string,
  destinationDirectory: string,
): Promise<void> {
  await copyDirectoryRecursive(sourceDirectory, destinationDirectory);
}

async function isManagedSkill(skillDirectory: string): Promise<boolean> {
  const [skillFileManaged, readmeFileManaged] = await Promise.all([
    fileContainsManagedMarker(join(skillDirectory, "SKILL.md")),
    fileContainsManagedMarker(join(skillDirectory, "README.md")),
  ]);
  return skillFileManaged && readmeFileManaged;
}

async function fileContainsManagedMarker(filePath: string): Promise<boolean> {
  try {
    const content = await readFile(filePath, "utf8");
    return content.includes(MANAGED_MARKER);
  } catch (error) {
    if (hasErrorCode(error, "ENOENT")) {
      return false;
    }
    throw error;
  }
}

async function inspectPath(path: string): Promise<PathKind> {
  try {
    const pathStats = await stat(path);
    return pathStats.isDirectory() ? "directory" : "file";
  } catch (error) {
    if (hasErrorCode(error, "ENOENT")) {
      return "missing";
    }
    throw error;
  }
}

async function resolveManagedSkillSourceDirectory(
  skillName: string,
  sourceRootDir?: string,
): Promise<string> {
  const defaultHome = homedir();
  const moduleDirectory = dirname(fileURLToPath(import.meta.url));
  const processArgv0Directory = process.argv[0] ? dirname(resolve(process.argv[0])) : undefined;
  const processArgvDirectory = process.argv[1] ? dirname(resolve(process.argv[1])) : undefined;
  const execDirectory = dirname(process.execPath);
  const candidates = uniqueNonEmptyPaths([
    sourceRootDir ? normalizeDirectory(sourceRootDir, defaultHome) : undefined,
    process.env.TSQ_SKILLS_DIR
      ? normalizeDirectory(process.env.TSQ_SKILLS_DIR, defaultHome)
      : undefined,
    resolve(process.cwd(), "SKILLS"),
    processArgv0Directory ? join(processArgv0Directory, "SKILLS") : undefined,
    processArgv0Directory ? resolve(processArgv0Directory, "..", "SKILLS") : undefined,
    processArgvDirectory ? join(processArgvDirectory, "SKILLS") : undefined,
    resolve(moduleDirectory, "..", "..", "SKILLS"),
    join(execDirectory, "SKILLS"),
    resolve(execDirectory, "..", "SKILLS"),
    join(execDirectory, "..", "share", "tsq", "SKILLS"),
  ]);

  for (const candidateRoot of candidates) {
    const candidateDirectory = join(candidateRoot, skillName);
    const pathKind = await inspectPath(candidateDirectory);
    if (pathKind !== "directory") {
      continue;
    }
    const skillFileKind = await inspectPath(join(candidateDirectory, "SKILL.md"));
    if (skillFileKind === "file") {
      return candidateDirectory;
    }
  }

  throw new TsqError(
    "VALIDATION_ERROR",
    `skill source not found for '${skillName}' (expected SKILLS/${skillName}/SKILL.md)`,
    1,
    { searched_roots: candidates, skill_name: skillName },
  );
}

async function copyDirectoryRecursive(
  sourceDirectory: string,
  destinationDirectory: string,
): Promise<void> {
  await mkdir(destinationDirectory, { recursive: true });
  const entries = await readdir(sourceDirectory, { withFileTypes: true });

  for (const entry of entries) {
    const sourcePath = join(sourceDirectory, entry.name);
    const destinationPath = join(destinationDirectory, entry.name);
    if (entry.isDirectory()) {
      await copyDirectoryRecursive(sourcePath, destinationPath);
      continue;
    }
    if (entry.isFile()) {
      await copyFile(sourcePath, destinationPath);
      continue;
    }
    throw new TsqError(
      "VALIDATION_ERROR",
      `unsupported entry in managed skill source: ${sourcePath}`,
      1,
    );
  }
}

function uniqueNonEmptyPaths(paths: Array<string | undefined>): string[] {
  const unique = new Set<string>();
  for (const path of paths) {
    if (!path || path.length === 0) {
      continue;
    }
    unique.add(path);
  }
  return [...unique];
}

function hasErrorCode(error: unknown, code: string): boolean {
  if (typeof error !== "object" || error === null) {
    return false;
  }
  return (error as { code?: unknown }).code === code;
}
