pub mod embedded;
pub mod helpers;
pub mod managed;
pub mod types;

use crate::errors::TsqError;
use crate::skills::embedded::materialize_embedded_skill;
use crate::skills::helpers::{
    PathKind, copy_directory_recursive, inspect_path, io_error_value, normalize_directory,
};
use crate::skills::managed::MANAGED_MARKER;
use crate::skills::types::{
    SkillAction, SkillOperationOptions, SkillOperationResult, SkillOperationSummary,
    SkillResultStatus, SkillTarget,
};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use ulid::Ulid;

pub fn apply_skill_operation(
    options: SkillOperationOptions,
) -> Result<SkillOperationSummary, TsqError> {
    let target_directories = resolve_target_directories(&options)?;
    let needs_source = matches!(options.action, SkillAction::Install | SkillAction::Refresh);
    let mut embedded_temp_root: Option<PathBuf> = None;
    let skill_source_directory = if needs_source {
        match resolve_managed_skill_source_directory(
            &options.skill_name,
            options.source_root_dir.as_deref(),
        ) {
            Ok(path) => Some(path),
            Err(error) => {
                if error.code != "VALIDATION_ERROR" {
                    return Err(error);
                }
                let searched_details = error.details.clone();
                let materialized = match materialize_embedded_skill(&options.skill_name) {
                    Ok(materialized) => materialized,
                    Err(embed_error) => {
                        if let Some(details) = searched_details {
                            return Err(embed_error.with_details(details));
                        }
                        return Err(embed_error);
                    }
                };
                embedded_temp_root = Some(materialized.temp_root.clone());
                Some(materialized.skill_root)
            }
        }
    } else {
        None
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
                results.push(install_skill(
                    *target,
                    &options.skill_name,
                    source,
                    &skill_directory,
                    options.force,
                )?);
            }
            SkillAction::Uninstall => {
                results.push(uninstall_skill(*target, &skill_directory, options.force)?);
            }
            SkillAction::Refresh => {
                let source = skill_source_directory.as_ref().ok_or_else(|| {
                    TsqError::new("INTERNAL_ERROR", "missing managed skill source", 2)
                })?;
                results.push(refresh_skill(*target, source, &skill_directory)?);
            }
        }
    }

    if let Some(temp_root) = embedded_temp_root {
        let _ = fs::remove_dir_all(temp_root);
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

    let defaults: HashMap<SkillTarget, PathBuf> = HashMap::from([
        (
            SkillTarget::Claude,
            resolved_home.join(".claude").join("skills"),
        ),
        (SkillTarget::Codex, resolved_codex_home.join("skills")),
        (
            SkillTarget::Copilot,
            resolved_home.join(".copilot").join("skills"),
        ),
        (
            SkillTarget::Opencode,
            resolved_home.join(".opencode").join("skills"),
        ),
    ]);

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

fn skill_result(
    target: SkillTarget,
    path: &Path,
    status: SkillResultStatus,
    message: &str,
) -> SkillOperationResult {
    SkillOperationResult {
        target,
        path: path.display().to_string(),
        status,
        message: Some(message.to_string()),
    }
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
        copy_directory_recursive(skill_source_directory, skill_directory)?;
        return Ok(skill_result(
            target,
            skill_directory,
            SkillResultStatus::Installed,
            "installed new managed skill",
        ));
    }

    if matches!(path_kind, PathKind::File | PathKind::Symlink) {
        if !force {
            return Ok(skill_result(
                target,
                skill_directory,
                SkillResultStatus::Skipped,
                "path exists as a non-directory and force is disabled",
            ));
        }
        fs::remove_file(skill_directory).map_err(|e| {
            TsqError::new("IO_ERROR", "failed removing existing skill path", 2)
                .with_details(io_error_value(&e))
        })?;
        copy_directory_recursive(skill_source_directory, skill_directory)?;
        return Ok(skill_result(
            target,
            skill_directory,
            SkillResultStatus::Updated,
            "replaced non-directory path with managed skill due to force",
        ));
    }

    let managed = is_managed_skill(skill_directory)?;
    if !managed && !force {
        return Ok(skill_result(
            target,
            skill_directory,
            SkillResultStatus::Skipped,
            "existing skill is not managed and force is disabled",
        ));
    }

    fs::remove_dir_all(skill_directory).map_err(|e| {
        TsqError::new("IO_ERROR", "failed removing existing skill directory", 2)
            .with_details(io_error_value(&e))
    })?;
    copy_directory_recursive(skill_source_directory, skill_directory)?;
    Ok(skill_result(
        target,
        skill_directory,
        SkillResultStatus::Updated,
        if managed {
            "updated managed skill"
        } else {
            "overwrote non-managed skill due to force"
        },
    ))
}

fn uninstall_skill(
    target: SkillTarget,
    skill_directory: &Path,
    force: bool,
) -> Result<SkillOperationResult, TsqError> {
    let path_kind = inspect_path(skill_directory)?;
    if path_kind == PathKind::Missing {
        return Ok(skill_result(
            target,
            skill_directory,
            SkillResultStatus::NotFound,
            "skill directory not found",
        ));
    }

    if matches!(path_kind, PathKind::File | PathKind::Symlink) {
        if !force {
            return Ok(skill_result(
                target,
                skill_directory,
                SkillResultStatus::Skipped,
                "path exists as a non-directory and force is disabled",
            ));
        }
        fs::remove_file(skill_directory).map_err(|e| {
            TsqError::new("IO_ERROR", "failed removing non-directory skill path", 2)
                .with_details(io_error_value(&e))
        })?;
        return Ok(skill_result(
            target,
            skill_directory,
            SkillResultStatus::Removed,
            "removed non-directory path due to force",
        ));
    }

    let managed = is_managed_skill(skill_directory)?;
    if !managed && !force {
        return Ok(skill_result(
            target,
            skill_directory,
            SkillResultStatus::Skipped,
            "existing skill is not managed and force is disabled",
        ));
    }

    fs::remove_dir_all(skill_directory).map_err(|e| {
        TsqError::new("IO_ERROR", "failed removing skill directory", 2)
            .with_details(io_error_value(&e))
    })?;
    Ok(skill_result(
        target,
        skill_directory,
        SkillResultStatus::Removed,
        if managed {
            "removed managed skill"
        } else {
            "removed non-managed skill due to force"
        },
    ))
}

fn refresh_skill(
    target: SkillTarget,
    skill_source_directory: &Path,
    skill_directory: &Path,
) -> Result<SkillOperationResult, TsqError> {
    let path_kind = inspect_path(skill_directory)?;

    if path_kind == PathKind::Missing {
        return Ok(skill_result(
            target,
            skill_directory,
            SkillResultStatus::NotFound,
            "skill directory not found",
        ));
    }
    if matches!(path_kind, PathKind::File | PathKind::Symlink) {
        return Ok(skill_result(
            target,
            skill_directory,
            SkillResultStatus::Skipped,
            "target is a file, not a managed skill directory",
        ));
    }

    let managed = is_managed_skill(skill_directory)?;
    if !managed {
        return Ok(skill_result(
            target,
            skill_directory,
            SkillResultStatus::Skipped,
            "target is not a managed skill",
        ));
    }

    // Atomic-ish refresh via backup sibling. The individual renames below are
    // atomic on POSIX filesystems, but the whole sequence is not transactional:
    // a process crash between steps can require manual cleanup of temp/backup dirs.
    //   1. Copy source to temp sibling
    //   2. Rename existing skill dir to backup
    //   3a. Happy path: rename temp into final location
    //   3b. Rename failure: clean temp and restore backup before returning error
    //   4. On success, remove backup
    let ulid = Ulid::new();
    let parent = skill_directory
        .parent()
        .ok_or_else(|| TsqError::new("INTERNAL_ERROR", "skill directory has no parent", 2))?;
    let temp_path = parent.join(format!(".tsq-refresh-{}", ulid));
    let backup_path = parent.join(format!(".tsq-refresh-backup-{}", ulid));

    // Step 1: Copy source to temp. Old dir untouched if this fails.
    if let Err(e) = copy_directory_recursive(skill_source_directory, &temp_path) {
        let _ = fs::remove_dir_all(&temp_path);
        return Err(e);
    }
    let existing_permissions = fs::metadata(skill_directory)
        .map_err(|e| {
            TsqError::new("IO_ERROR", "failed reading existing skill permissions", 2)
                .with_details(io_error_value(&e))
        })?
        .permissions();
    if let Err(e) = fs::set_permissions(&temp_path, existing_permissions) {
        let _ = fs::remove_dir_all(&temp_path);
        return Err(
            TsqError::new("IO_ERROR", "failed setting refreshed skill permissions", 2)
                .with_details(io_error_value(&e)),
        );
    }

    // Step 2: Rename existing skill dir to backup.
    if let Err(e) = fs::rename(skill_directory, &backup_path) {
        let _ = fs::remove_dir_all(&temp_path);
        return Err(
            TsqError::new("IO_ERROR", "failed backing up existing skill directory", 2)
                .with_details(io_error_value(&e)),
        );
    }

    // Step 3: Rename temp into final location.
    if let Err(e) = fs::rename(&temp_path, skill_directory) {
        // Step 3 failed — restore backup to original location before returning error.
        let cleanup_error = fs::remove_dir_all(&temp_path).err();
        let restore_error = fs::rename(&backup_path, skill_directory).err();
        return Err(
            TsqError::new("IO_ERROR", "failed replacing managed skill directory", 2).with_details(
                serde_json::json!({
                    "backup_path": backup_path.display().to_string(),
                    "skill_directory": skill_directory.display().to_string(),
                    "recovery": "if restore_error is present, manually move backup_path back to skill_directory",
                    "replace_error": io_error_value(&e),
                    "cleanup_error": cleanup_error.as_ref().map(io_error_value),
                    "restore_error": restore_error.as_ref().map(io_error_value),
                }),
            ),
        );
    }

    // Step 4: Success — remove backup best-effort.
    let _ = fs::remove_dir_all(&backup_path);

    Ok(skill_result(
        target,
        skill_directory,
        SkillResultStatus::Updated,
        "refreshed managed skill",
    ))
}

fn is_managed_skill(skill_directory: &Path) -> Result<bool, TsqError> {
    file_contains_managed_marker(&skill_directory.join("SKILL.md"))
}

fn file_contains_managed_marker(file_path: &Path) -> Result<bool, TsqError> {
    match fs::read_to_string(file_path) {
        Ok(content) => Ok(content.contains(MANAGED_MARKER)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(TsqError::new("IO_ERROR", "failed reading skill file", 2)
            .with_details(io_error_value(&e))),
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
    if let Ok(exe_path) = env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        roots.push(exe_dir.join("SKILLS"));
        roots.push(exe_dir.join("..").join("SKILLS"));
        roots.push(exe_dir.join("..").join("share").join("tsq").join("SKILLS"));
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

#[cfg(test)]
mod tests;
