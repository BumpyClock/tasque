export type SkillTarget = "claude" | "codex" | "copilot" | "opencode";
export type SkillAction = "install" | "uninstall";
export type SkillResultStatus = "installed" | "updated" | "skipped" | "removed" | "not_found";

export interface SkillOperationOptions {
  action: SkillAction;
  skillName: string;
  targets: SkillTarget[];
  force: boolean;
  sourceRootDir?: string;
  homeDir?: string;
  codexHome?: string;
  targetDirOverrides?: Partial<Record<SkillTarget, string>>;
}

export interface SkillOperationResult {
  target: SkillTarget;
  path: string;
  status: SkillResultStatus;
  message?: string;
}

export interface SkillOperationSummary {
  action: SkillAction;
  skill_name: string;
  results: SkillOperationResult[];
}
