use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillTarget {
    Claude,
    Codex,
    Copilot,
    Opencode,
}

impl SkillTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            SkillTarget::Claude => "claude",
            SkillTarget::Codex => "codex",
            SkillTarget::Copilot => "copilot",
            SkillTarget::Opencode => "opencode",
        }
    }
}

impl fmt::Display for SkillTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillAction {
    Install,
    Uninstall,
    Refresh,
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

impl SkillResultStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SkillResultStatus::Installed => "installed",
            SkillResultStatus::Updated => "updated",
            SkillResultStatus::Skipped => "skipped",
            SkillResultStatus::Removed => "removed",
            SkillResultStatus::NotFound => "not_found",
        }
    }
}

impl fmt::Display for SkillResultStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
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
