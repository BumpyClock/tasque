pub mod managed;
pub mod types;

use crate::errors::TsqError;
use crate::skills::managed::MANAGED_MARKER;
use crate::skills::types::{
    SkillAction, SkillOperationOptions, SkillOperationResult, SkillOperationSummary,
    SkillResultStatus, SkillTarget,
};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathKind {
    Missing,
    File,
    Directory,
}

pub fn apply_skill_operation(
    options: SkillOperationOptions,
) -> Result<SkillOperationSummary, TsqError> {
    let target_directories = resolve_target_directories(&options)?;
    let skill_source_directory = match options.action {
        SkillAction::Install => Some(resolve_managed_skill_source_directory(
            &options.skill_name,
            options.source_root_dir.as_deref(),
        )?),
        SkillAction::Uninstall => None,
    };

    let mut results = Vec::new();
    for target in &options.targets {
        let target_directory = target_directories
            .get(target)
            .ok_or_else(|| TsqError::new("INTERNAL_ERROR", "missing target directory", 2))?;
        let skill_directory = target_directory.join(&options.skill_name);
        match options.action {
            SkillAction::Install => {
                let source = skill_source_directory.as_ref().ok_or_else(|| {
                    TsqError::new("INTERNAL_ERROR", "missing managed skill source", 2)
                })?;
                let result = install_skill(
                    *target,
                    &options.skill_name,
                    source,
                    &skill_directory,
                    options.force,
                )?;
                results.push(result);
            }
            SkillAction::Uninstall => {
                let result = uninstall_skill(*target, &skill_directory, options.force)?;
                results.push(result);
            }
        }
    }

    Ok(SkillOperationSummary {
        action: options.action,
        skill_name: options.skill_name,
        results,
    })
}

fn resolve_target_directories(
    options: &SkillOperationOptions,
) -> Result<HashMap<SkillTarget, PathBuf>, TsqError> {
    let default_home =
        dirs::home_dir().ok_or_else(|| TsqError::new("IO_ERROR", "home directory not found", 2))?;
    let resolved_home = normalize_directory(
        options
            .home_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or(default_home.clone()),
        &default_home,
    )?;

    let raw_codex_home = options
        .codex_home
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| env::var("CODEX_HOME").ok().map(PathBuf::from))
        .unwrap_or_else(|| resolved_home.join(".codex"));
    let resolved_codex_home = normalize_directory(raw_codex_home, &resolved_home)?;

    let mut defaults: HashMap<SkillTarget, PathBuf> = HashMap::new();
    defaults.insert(
        SkillTarget::Claude,
        resolved_home.join(".claude").join("skills"),
    );
    defaults.insert(SkillTarget::Codex, resolved_codex_home.join("skills"));
    defaults.insert(
        SkillTarget::Copilot,
        resolved_home.join(".copilot").join("skills"),
    );
    defaults.insert(
        SkillTarget::Opencode,
        resolved_home.join(".opencode").join("skills"),
    );

    let mut result = HashMap::new();
    for target in [
        SkillTarget::Claude,
        SkillTarget::Codex,
        SkillTarget::Copilot,
        SkillTarget::Opencode,
    ] {
        let override_dir = options
            .target_dir_overrides
            .as_ref()
            .and_then(|map| map.get(&target))
            .map(PathBuf::from);
        let directory = override_dir.unwrap_or_else(|| defaults[&target].clone());
        result.insert(target, normalize_directory(directory, &resolved_home)?);
    }

    Ok(result)
}

fn normalize_directory(directory: PathBuf, home: &Path) -> Result<PathBuf, TsqError> {
    let expanded = expand_home(&directory, home);
    if expanded.is_absolute() {
        return Ok(expanded);
    }
    let cwd = env::current_dir().map_err(|e| {
        TsqError::new("IO_ERROR", "failed resolving current directory", 2)
            .with_details(io_error_value(&e))
    })?;
    Ok(cwd.join(expanded))
}

fn expand_home(path: &Path, home: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if raw == "~" {
        return home.to_path_buf();
    }
    if raw.starts_with("~/") || raw.starts_with("~\\") {
        return home.join(&raw[2..]);
    }
    path.to_path_buf()
}

fn install_skill(
    target: SkillTarget,
    _skill_name: &str,
    skill_source_directory: &Path,
    skill_directory: &Path,
    force: bool,
) -> Result<SkillOperationResult, TsqError> {
    let path_kind = inspect_path(skill_directory)?;
    if path_kind == PathKind::Missing {
        copy_managed_skill_directory(skill_source_directory, skill_directory)?;
        return Ok(SkillOperationResult {
            target,
            path: skill_directory.display().to_string(),
            status: SkillResultStatus::Installed,
            message: Some("installed new managed skill".to_string()),
        });
    }

    if path_kind == PathKind::File {
        if !force {
            return Ok(SkillOperationResult {
                target,
                path: skill_directory.display().to_string(),
                status: SkillResultStatus::Skipped,
                message: Some("path exists as a non-directory and force is disabled".to_string()),
            });
        }
        fs::remove_file(skill_directory).map_err(|e| {
            TsqError::new("IO_ERROR", "failed removing existing skill path", 2)
                .with_details(io_error_value(&e))
        })?;
        copy_managed_skill_directory(skill_source_directory, skill_directory)?;
        return Ok(SkillOperationResult {
            target,
            path: skill_directory.display().to_string(),
            status: SkillResultStatus::Updated,
            message: Some(
                "replaced non-directory path with managed skill due to force".to_string(),
            ),
        });
    }

    let managed = is_managed_skill(skill_directory)?;
    if !managed && !force {
        return Ok(SkillOperationResult {
            target,
            path: skill_directory.display().to_string(),
            status: SkillResultStatus::Skipped,
            message: Some("existing skill is not managed and force is disabled".to_string()),
        });
    }

    fs::remove_dir_all(skill_directory).map_err(|e| {
        TsqError::new("IO_ERROR", "failed removing existing skill directory", 2)
            .with_details(io_error_value(&e))
    })?;
    copy_managed_skill_directory(skill_source_directory, skill_directory)?;
    Ok(SkillOperationResult {
        target,
        path: skill_directory.display().to_string(),
        status: SkillResultStatus::Updated,
        message: Some(
            if managed {
                "updated managed skill"
            } else {
                "overwrote non-managed skill due to force"
            }
            .to_string(),
        ),
    })
}

fn uninstall_skill(
    target: SkillTarget,
    skill_directory: &Path,
    force: bool,
) -> Result<SkillOperationResult, TsqError> {
    let path_kind = inspect_path(skill_directory)?;
    if path_kind == PathKind::Missing {
        return Ok(SkillOperationResult {
            target,
            path: skill_directory.display().to_string(),
            status: SkillResultStatus::NotFound,
            message: Some("skill directory not found".to_string()),
        });
    }

    if path_kind == PathKind::File {
        if !force {
            return Ok(SkillOperationResult {
                target,
                path: skill_directory.display().to_string(),
                status: SkillResultStatus::Skipped,
                message: Some("path exists as a non-directory and force is disabled".to_string()),
            });
        }
        fs::remove_file(skill_directory).map_err(|e| {
            TsqError::new("IO_ERROR", "failed removing non-directory skill path", 2)
                .with_details(io_error_value(&e))
        })?;
        return Ok(SkillOperationResult {
            target,
            path: skill_directory.display().to_string(),
            status: SkillResultStatus::Removed,
            message: Some("removed non-directory path due to force".to_string()),
        });
    }

    let managed = is_managed_skill(skill_directory)?;
    if !managed && !force {
        return Ok(SkillOperationResult {
            target,
            path: skill_directory.display().to_string(),
            status: SkillResultStatus::Skipped,
            message: Some("existing skill is not managed and force is disabled".to_string()),
        });
    }

    fs::remove_dir_all(skill_directory).map_err(|e| {
        TsqError::new("IO_ERROR", "failed removing skill directory", 2)
            .with_details(io_error_value(&e))
    })?;
    Ok(SkillOperationResult {
        target,
        path: skill_directory.display().to_string(),
        status: SkillResultStatus::Removed,
        message: Some(
            if managed {
                "removed managed skill"
            } else {
                "removed non-managed skill due to force"
            }
            .to_string(),
        ),
    })
}

fn copy_managed_skill_directory(
    source_directory: &Path,
    destination_directory: &Path,
) -> Result<(), TsqError> {
    copy_directory_recursive(source_directory, destination_directory)
}

fn is_managed_skill(skill_directory: &Path) -> Result<bool, TsqError> {
    let skill_file_managed = file_contains_managed_marker(&skill_directory.join("SKILL.md"))?;
    let readme_file_managed = file_contains_managed_marker(&skill_directory.join("README.md"))?;
    Ok(skill_file_managed && readme_file_managed)
}

fn file_contains_managed_marker(file_path: &Path) -> Result<bool, TsqError> {
    match fs::read_to_string(file_path) {
        Ok(content) => Ok(content.contains(MANAGED_MARKER)),
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return Ok(false);
            }
            Err(TsqError::new("IO_ERROR", "failed reading skill file", 2)
                .with_details(io_error_value(&error)))
        }
    }
}

fn inspect_path(path: &Path) -> Result<PathKind, TsqError> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                Ok(PathKind::Directory)
            } else {
                Ok(PathKind::File)
            }
        }
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return Ok(PathKind::Missing);
            }
            Err(TsqError::new("IO_ERROR", "failed to inspect skill path", 2)
                .with_details(io_error_value(&error)))
        }
    }
}

fn resolve_managed_skill_source_directory(
    skill_name: &str,
    source_root_dir: Option<&str>,
) -> Result<PathBuf, TsqError> {
    let default_home =
        dirs::home_dir().ok_or_else(|| TsqError::new("IO_ERROR", "home directory not found", 2))?;

    let mut roots: Vec<PathBuf> = Vec::new();

    if let Some(dir) = source_root_dir {
        roots.push(normalize_directory(PathBuf::from(dir), &default_home)?);
    }
    if let Ok(dir) = env::var("TSQ_SKILLS_DIR") {
        roots.push(normalize_directory(PathBuf::from(dir), &default_home)?);
    }
    if let Ok(cwd) = env::current_dir() {
        roots.push(cwd.join("SKILLS"));
    }
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            roots.push(exe_dir.join("SKILLS"));
            roots.push(exe_dir.join("..").join("SKILLS"));
            roots.push(exe_dir.join("..").join("share").join("tsq").join("SKILLS"));
        }
    }

    let mut unique_roots = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
        let key = root.to_string_lossy().to_string();
        if seen.insert(key) {
            unique_roots.push(root);
        }
    }

    for root in &unique_roots {
        let candidate_directory = root.join(skill_name);
        if inspect_path(&candidate_directory)? != PathKind::Directory {
            continue;
        }
        if inspect_path(&candidate_directory.join("SKILL.md"))? == PathKind::File {
            return Ok(candidate_directory);
        }
    }

    let searched_roots: Vec<String> = unique_roots
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    Err(TsqError::new(
        "VALIDATION_ERROR",
        format!(
            "skill source not found for '{}' (expected SKILLS/{}/SKILL.md)",
            skill_name, skill_name
        ),
        1,
    )
    .with_details(serde_json::json!({
      "searched_roots": searched_roots,
      "skill_name": skill_name,
    })))
}

fn copy_directory_recursive(
    source_directory: &Path,
    destination_directory: &Path,
) -> Result<(), TsqError> {
    fs::create_dir_all(destination_directory).map_err(|e| {
        TsqError::new("IO_ERROR", "failed creating destination skill directory", 2)
            .with_details(io_error_value(&e))
    })?;
    let entries = fs::read_dir(source_directory).map_err(|e| {
        TsqError::new("IO_ERROR", "failed reading source skill directory", 2)
            .with_details(io_error_value(&e))
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| {
            TsqError::new("IO_ERROR", "failed reading source directory entry", 2)
                .with_details(io_error_value(&e))
        })?;
        let source_path = entry.path();
        let destination_path = destination_directory.join(entry.file_name());
        let file_type = entry.file_type().map_err(|e| {
            TsqError::new("IO_ERROR", "failed reading source entry file type", 2)
                .with_details(io_error_value(&e))
        })?;

        if file_type.is_dir() {
            copy_directory_recursive(&source_path, &destination_path)?;
            continue;
        }
        if file_type.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|e| {
                TsqError::new("IO_ERROR", "failed copying managed skill file", 2)
                    .with_details(io_error_value(&e))
            })?;
            continue;
        }
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            format!(
                "unsupported entry in managed skill source: {}",
                source_path.display()
            ),
            1,
        ));
    }

    Ok(())
}

fn io_error_value(error: &std::io::Error) -> serde_json::Value {
    serde_json::json!({"kind": format!("{:?}", error.kind()), "message": error.to_string()})
}
