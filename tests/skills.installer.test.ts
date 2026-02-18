import { afterEach, describe, expect, it } from "bun:test";
import { mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { applySkillOperation } from "../src/skills";
import type { SkillOperationResult, SkillTarget } from "../src/skills/types";

const MANAGED_MARKER = "tsq-managed-skill:v1";
const tempDirectories: string[] = [];
const targets: SkillTarget[] = ["claude", "codex", "copilot", "opencode"];

afterEach(async () => {
  await Promise.all(
    tempDirectories.splice(0).map((directory) => rm(directory, { recursive: true, force: true })),
  );
});

async function makeTempDirectory(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), "tasque-skills-"));
  tempDirectories.push(directory);
  return directory;
}

function resultForTarget(
  results: SkillOperationResult[],
  target: SkillTarget,
): SkillOperationResult {
  const result = results.find((candidate) => candidate.target === target);
  expect(result).toBeDefined();
  return result as SkillOperationResult;
}

async function pathExists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

describe("skills installer", () => {
  it("installs updates and uninstalls managed skills for all targets", async () => {
    const root = await makeTempDirectory();
    const homeDirectory = join(root, "home");
    const codexHomeDirectory = join(root, "codex-home");
    const targetDirOverrides = {
      claude: join(root, "targets", "claude-skills"),
      codex: join(root, "targets", "codex-skills"),
      copilot: join(root, "targets", "copilot-skills"),
      opencode: join(root, "targets", "opencode-skills"),
    } satisfies Partial<Record<SkillTarget, string>>;

    const installSummary = await applySkillOperation({
      action: "install",
      skillName: "tasque",
      targets,
      force: false,
      homeDir: homeDirectory,
      codexHome: codexHomeDirectory,
      targetDirOverrides,
    });

    expect(installSummary.action).toBe("install");
    expect(installSummary.skill_name).toBe("tasque");
    expect(installSummary.results).toHaveLength(4);

    for (const target of targets) {
      const result = resultForTarget(installSummary.results, target);
      const expectedPath = join(targetDirOverrides[target] as string, "tasque");

      expect(result.path).toBe(expectedPath);
      expect(result.status).toBe("installed");

      const skillMarkdownPath = join(expectedPath, "SKILL.md");
      const skillMarkdown = await readFile(skillMarkdownPath, "utf8");
      expect(skillMarkdown.startsWith("---")).toBe(true);
      expect(skillMarkdown.includes(MANAGED_MARKER)).toBe(true);
      expect(skillMarkdown.includes("tsq ready")).toBe(true);
      expect(skillMarkdown.includes("tsq create")).toBe(true);
      expect(skillMarkdown.includes("tsq update")).toBe(true);
      expect(skillMarkdown.includes("tsq dep add")).toBe(true);
      expect(skillMarkdown.includes("tsq show")).toBe(true);
      expect(skillMarkdown.includes("tsq link add")).toBe(true);
      expect(skillMarkdown.includes("tsq supersede")).toBe(true);
      expect(skillMarkdown.includes("tsq doctor")).toBe(true);
      expect(skillMarkdown.includes("--json")).toBe(true);
      expect(skillMarkdown.includes("gate readiness")).toBe(true);
      expect(skillMarkdown.includes(".tasque/events.jsonl")).toBe(true);
    }

    const updateSummary = await applySkillOperation({
      action: "install",
      skillName: "tasque",
      targets,
      force: false,
      homeDir: homeDirectory,
      codexHome: codexHomeDirectory,
      targetDirOverrides,
    });

    for (const target of targets) {
      expect(resultForTarget(updateSummary.results, target).status).toBe("updated");
    }

    const uninstallSummary = await applySkillOperation({
      action: "uninstall",
      skillName: "tasque",
      targets,
      force: false,
      homeDir: homeDirectory,
      codexHome: codexHomeDirectory,
      targetDirOverrides,
    });

    expect(uninstallSummary.action).toBe("uninstall");
    expect(uninstallSummary.results).toHaveLength(4);

    for (const target of targets) {
      const result = resultForTarget(uninstallSummary.results, target);
      const expectedPath = join(targetDirOverrides[target] as string, "tasque");
      expect(result.path).toBe(expectedPath);
      expect(result.status).toBe("removed");
      expect(await pathExists(expectedPath)).toBe(false);
    }
  });

  it("skips install and uninstall for non-managed skills when force is disabled", async () => {
    const root = await makeTempDirectory();
    const targetDirectory = join(root, "claude-target");
    const skillDirectory = join(targetDirectory, "tasque");

    await mkdir(skillDirectory, { recursive: true });
    await writeFile(join(skillDirectory, "SKILL.md"), "# user skill\n", "utf8");
    await writeFile(join(skillDirectory, "README.md"), "custom docs\n", "utf8");

    const installSummary = await applySkillOperation({
      action: "install",
      skillName: "tasque",
      targets: ["claude"],
      force: false,
      homeDir: join(root, "home"),
      codexHome: join(root, "codex-home"),
      targetDirOverrides: { claude: targetDirectory },
    });

    expect(installSummary.results).toHaveLength(1);
    expect(installSummary.results[0]?.status).toBe("skipped");
    expect(await readFile(join(skillDirectory, "SKILL.md"), "utf8")).toBe("# user skill\n");

    const uninstallSummary = await applySkillOperation({
      action: "uninstall",
      skillName: "tasque",
      targets: ["claude"],
      force: false,
      homeDir: join(root, "home"),
      codexHome: join(root, "codex-home"),
      targetDirOverrides: { claude: targetDirectory },
    });

    expect(uninstallSummary.results).toHaveLength(1);
    expect(uninstallSummary.results[0]?.status).toBe("skipped");
    expect(await pathExists(skillDirectory)).toBe(true);
  });

  it("force-overwrites non-managed skills and force-removes unmanaged skills", async () => {
    const root = await makeTempDirectory();
    const targetDirectory = join(root, "copilot-target");
    const skillDirectory = join(targetDirectory, "tasque");

    await mkdir(skillDirectory, { recursive: true });
    await writeFile(join(skillDirectory, "SKILL.md"), "# custom skill\n", "utf8");
    await writeFile(join(skillDirectory, "README.md"), "custom readme\n", "utf8");

    const forceInstall = await applySkillOperation({
      action: "install",
      skillName: "tasque",
      targets: ["copilot"],
      force: true,
      homeDir: join(root, "home"),
      codexHome: join(root, "codex-home"),
      targetDirOverrides: { copilot: targetDirectory },
    });

    expect(forceInstall.results).toHaveLength(1);
    expect(forceInstall.results[0]?.status).toBe("updated");

    const skillMarkdown = await readFile(join(skillDirectory, "SKILL.md"), "utf8");
    expect(skillMarkdown.includes(MANAGED_MARKER)).toBe(true);

    const unmanagedSkillDirectory = join(targetDirectory, "custom-skill");
    await mkdir(unmanagedSkillDirectory, { recursive: true });
    await writeFile(join(unmanagedSkillDirectory, "SKILL.md"), "# unmanaged\n", "utf8");

    const forceUninstall = await applySkillOperation({
      action: "uninstall",
      skillName: "custom-skill",
      targets: ["copilot"],
      force: true,
      homeDir: join(root, "home"),
      codexHome: join(root, "codex-home"),
      targetDirOverrides: { copilot: targetDirectory },
    });

    expect(forceUninstall.results).toHaveLength(1);
    expect(forceUninstall.results[0]?.status).toBe("removed");
    expect(await pathExists(unmanagedSkillDirectory)).toBe(false);
  });
});
