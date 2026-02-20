use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillTarget {
    Claude,
    Codex,
    Copilot,
    Opencode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillAction {
    Install,
    Uninstall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillResultStatus {
    Installed,
    Updated,
    Skipped,
    Removed,
    NotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOperationOptions {
    pub action: SkillAction,
    pub skill_name: String,
    pub targets: Vec<SkillTarget>,
    pub force: bool,
    pub source_root_dir: Option<String>,
    pub home_dir: Option<String>,
    pub codex_home: Option<String>,
    pub target_dir_overrides: Option<HashMap<SkillTarget, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOperationResult {
    pub target: SkillTarget,
    pub path: String,
    pub status: SkillResultStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOperationSummary {
    pub action: SkillAction,
    pub skill_name: String,
    pub results: Vec<SkillOperationResult>,
}
