import { mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { isAbsolute, join, resolve } from "node:path";
import { MANAGED_MARKER, renderReadmeMarkdown, renderSkillMarkdown } from "./content";
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
  const results: SkillOperationResult[] = [];

  for (const target of options.targets) {
    const targetDirectory = targetDirectories[target];
    const skillDirectory = join(targetDirectory, options.skillName);

    if (options.action === "install") {
      results.push(
        await installSkill({
          force: options.force,
          skillName: options.skillName,
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
    await writeManagedSkillFiles(context.skillDirectory, context.skillName, context.target);
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
    await writeManagedSkillFiles(context.skillDirectory, context.skillName, context.target);
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

  await writeManagedSkillFiles(context.skillDirectory, context.skillName, context.target);
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

async function writeManagedSkillFiles(
  skillDirectory: string,
  skillName: string,
  target: SkillTarget,
): Promise<void> {
  await mkdir(skillDirectory, { recursive: true });
  await Promise.all([
    writeFile(join(skillDirectory, "SKILL.md"), renderSkillMarkdown(skillName), "utf8"),
    writeFile(join(skillDirectory, "README.md"), renderReadmeMarkdown(skillName, target), "utf8"),
  ]);
}

async function isManagedSkill(skillDirectory: string): Promise<boolean> {
  const [skillFileManaged, readmeFileManaged] = await Promise.all([
    fileContainsManagedMarker(join(skillDirectory, "SKILL.md")),
    fileContainsManagedMarker(join(skillDirectory, "README.md")),
  ]);
  return skillFileManaged || readmeFileManaged;
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

function hasErrorCode(error: unknown, code: string): boolean {
  if (typeof error !== "object" || error === null) {
    return false;
  }
  return (error as { code?: unknown }).code === code;
}
