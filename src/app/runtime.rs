use crate::errors::TsqError;
use crate::types::{Priority, TaskStatus};
use chrono::{SecondsFormat, Utc};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DEFAULT_SNAPSHOT_EVERY: usize = 100;

pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn find_tasque_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(".tasque").exists() {
            return Some(dir);
        }
        let parent = dir.parent()?.to_path_buf();
        if parent == dir {
            return None;
        }
        dir = parent;
    }
}

pub fn get_repo_root() -> PathBuf {
    find_tasque_root()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn get_actor(repo_root: impl AsRef<Path>) -> String {
    if let Ok(value) = std::env::var("TSQ_ACTOR") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    if let Some(name) = read_git_user_name(repo_root) {
        return name;
    }

    let os_user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .ok();
    if let Some(value) = os_user {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    "unknown".to_string()
}

pub fn parse_priority(raw: &str) -> Result<Priority, TsqError> {
    let value = raw
        .parse::<u8>()
        .map_err(|_| TsqError::new("VALIDATION_ERROR", "priority must be one of: 0, 1, 2, 3", 1))?;
    if matches!(value, 0..=3) {
        return Ok(value);
    }
    Err(TsqError::new(
        "VALIDATION_ERROR",
        "priority must be one of: 0, 1, 2, 3",
        1,
    ))
}

pub fn normalize_status(raw: &str) -> Result<TaskStatus, TsqError> {
    let normalized = match raw {
        "done" => "closed",
        "todo" => "open",
        _ => raw,
    };
    match normalized {
        "open" => Ok(TaskStatus::Open),
        "in_progress" => Ok(TaskStatus::InProgress),
        "blocked" => Ok(TaskStatus::Blocked),
        "closed" => Ok(TaskStatus::Closed),
        "canceled" => Ok(TaskStatus::Canceled),
        "deferred" => Ok(TaskStatus::Deferred),
        _ => Err(TsqError::new(
            "VALIDATION_ERROR",
            "status must be one of: open, todo, in_progress, blocked, closed, done, canceled, deferred",
            1,
        )),
    }
}

fn read_git_user_name(repo_root: impl AsRef<Path>) -> Option<String> {
    let output = Command::new("git")
        .args(["config", "user.name"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}
