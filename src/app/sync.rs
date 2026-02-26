use crate::errors::TsqError;
use crate::store::config::{read_config, write_config};
use crate::store::events::{append_events, read_events};
use crate::store::git;
use crate::store::paths::get_paths;
use crate::types::{MigrateResult, SyncSetupResult};
use std::collections::HashSet;
use std::path::Path;

/// Resolve the effective root directory for data operations.
///
/// If the config specifies a `sync_branch`, the data root is redirected to the
/// worktree for that branch. Otherwise returns `repo_root` unchanged.
pub fn resolve_effective_root(repo_root: &str) -> Result<String, TsqError> {
    let config = read_config(repo_root)?;

    let branch = match config.sync_branch {
        Some(branch) => branch,
        None => return Ok(repo_root.to_string()),
    };

    let repo_path = Path::new(repo_root);
    if !git::is_git_repo(repo_path) {
        return Err(TsqError::new(
            "GIT_NOT_AVAILABLE",
            "sync_branch is configured but repo is not a git repository",
            2,
        ));
    }

    let worktree = git::ensure_worktree(repo_path, &branch)?;
    Ok(worktree.to_string_lossy().to_string())
}

/// Set up sync branch infrastructure for a repository.
///
/// Creates the orphan branch, configures the merge driver, ensures the
/// worktree, and updates the config. Idempotent: returns early if the
/// config already matches the requested branch.
pub fn setup_sync_branch(
    repo_root: &str,
    branch: &str,
    _actor: &str,
) -> Result<SyncSetupResult, TsqError> {
    let repo_path = Path::new(repo_root);

    if !git::is_git_repo(repo_path) {
        return Err(TsqError::new(
            "GIT_NOT_AVAILABLE",
            "sync branch requires a git repository",
            2,
        ));
    }

    git::validate_branch_name(branch)?;

    let gitattributes_updated = git::ensure_gitattributes_entry(repo_path)?;
    git::setup_merge_driver_config(repo_path)?;

    let config = read_config(repo_root)?;
    if config.sync_branch.as_deref() == Some(branch) {
        let worktree = git::ensure_worktree(repo_path, branch)?;
        return Ok(SyncSetupResult {
            branch: branch.to_string(),
            worktree_path: worktree.to_string_lossy().to_string(),
            created_branch: false,
            merge_driver_configured: gitattributes_updated,
        });
    }

    let created_branch = if !git::branch_exists(repo_path, branch)? {
        let paths = get_paths(repo_root);
        ensure_seed_tasque_dir(&paths.tasque_dir)?;
        git::create_orphan_branch(repo_path, branch, &paths.tasque_dir)?;
        true
    } else {
        false
    };

    let worktree = git::ensure_worktree(repo_path, branch)?;

    let updated_config = crate::types::Config {
        sync_branch: Some(branch.to_string()),
        ..config
    };
    write_config(repo_root, &updated_config)?;

    Ok(SyncSetupResult {
        branch: branch.to_string(),
        worktree_path: worktree.to_string_lossy().to_string(),
        created_branch,
        merge_driver_configured: gitattributes_updated,
    })
}

/// Migrate existing events from the repo root into a sync branch.
///
/// Reads events from the current `.tasque/events.jsonl`, sets up the sync
/// branch, writes events to the worktree, and commits.
pub fn migrate_to_sync_branch(
    repo_root: &str,
    branch: &str,
    actor: &str,
) -> Result<MigrateResult, TsqError> {
    let existing = read_events(repo_root)?;

    let setup = setup_sync_branch(repo_root, branch, actor)?;

    let worktree_existing = read_events(&setup.worktree_path)?;
    let mut seen_ids = HashSet::new();
    for event in &worktree_existing.events {
        if let Some(id) = event.id.as_deref().or(event.event_id.as_deref()) {
            seen_ids.insert(id.to_string());
        }
    }

    let mut to_append = Vec::new();
    for event in &existing.events {
        let id = event.id.as_deref().or(event.event_id.as_deref());
        if let Some(id) = id {
            if !seen_ids.contains(id) {
                to_append.push(event.clone());
            }
        } else {
            to_append.push(event.clone());
        }
    }

    if !to_append.is_empty() {
        append_events(&setup.worktree_path, &to_append)?;
    }

    let wt_path = Path::new(&setup.worktree_path);
    git::commit_worktree(wt_path, "chore: migrate tasque events to sync branch")?;
    clear_repo_events(repo_root)?;

    Ok(MigrateResult {
        events_migrated: to_append.len(),
        branch: setup.branch,
        worktree_path: setup.worktree_path,
    })
}

fn ensure_seed_tasque_dir(tasque_dir: &Path) -> Result<(), TsqError> {
    std::fs::create_dir_all(tasque_dir).map_err(|e| {
        TsqError::new("IO_ERROR", "failed creating .tasque directory for seed", 2)
            .with_details(serde_json::json!({"message": e.to_string()}))
    })?;

    let events_file = tasque_dir.join("events.jsonl");
    if !events_file.exists() {
        std::fs::write(&events_file, "").map_err(|e| {
            TsqError::new("IO_ERROR", "failed creating seed events.jsonl", 2)
                .with_details(serde_json::json!({"message": e.to_string()}))
        })?;
    }

    let config_file = tasque_dir.join("config.json");
    if !config_file.exists() {
        let default = crate::types::Config {
            schema_version: crate::types::SCHEMA_VERSION,
            snapshot_every: 200,
            sync_branch: None,
        };
        let json = serde_json::to_string_pretty(&default).map_err(|e| {
            TsqError::new("IO_ERROR", "failed serializing seed config", 2)
                .with_details(serde_json::json!({"message": e.to_string()}))
        })?;
        std::fs::write(&config_file, format!("{}\n", json)).map_err(|e| {
            TsqError::new("IO_ERROR", "failed writing seed config.json", 2)
                .with_details(serde_json::json!({"message": e.to_string()}))
        })?;
    }

    let gitignore_file = tasque_dir.join(".gitignore");
    if !gitignore_file.exists() {
        let content = "state.json\nstate.json.tmp*\n.lock\nsnapshots/\nsnapshots/*.tmp\n";
        std::fs::write(&gitignore_file, content).map_err(|e| {
            TsqError::new("IO_ERROR", "failed writing seed .gitignore", 2)
                .with_details(serde_json::json!({"message": e.to_string()}))
        })?;
    }

    Ok(())
}

fn clear_repo_events(repo_root: &str) -> Result<(), TsqError> {
    let paths = get_paths(repo_root);
    std::fs::write(&paths.events_file, "").map_err(|e| {
        TsqError::new("EVENT_CLEAR_FAILED", "Failed clearing events", 2)
            .with_details(serde_json::json!({"message": e.to_string()}))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_effective_root_returns_repo_root_when_no_sync_branch() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = dir.path();
        std::fs::create_dir_all(repo.join(".tasque")).expect("mkdir");
        let config = crate::types::Config {
            schema_version: 1,
            snapshot_every: 200,
            sync_branch: None,
        };
        write_config(repo, &config).expect("write_config");

        let result =
            resolve_effective_root(&repo.to_string_lossy()).expect("resolve_effective_root");
        assert_eq!(result, repo.to_string_lossy().to_string());
    }

    #[test]
    fn resolve_effective_root_fails_when_sync_branch_set_but_not_git_repo() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = dir.path();
        std::fs::create_dir_all(repo.join(".tasque")).expect("mkdir");
        let config = crate::types::Config {
            schema_version: 1,
            snapshot_every: 200,
            sync_branch: Some("tasque-sync".to_string()),
        };
        write_config(repo, &config).expect("write_config");

        let err = resolve_effective_root(&repo.to_string_lossy()).expect_err("expected error");
        assert_eq!(err.code, "GIT_NOT_AVAILABLE");
    }

    #[test]
    fn setup_sync_branch_fails_on_non_git_repo() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = dir.path();
        std::fs::create_dir_all(repo.join(".tasque")).expect("mkdir");
        let config = crate::types::Config {
            schema_version: 1,
            snapshot_every: 200,
            sync_branch: None,
        };
        write_config(repo, &config).expect("write_config");

        let err = setup_sync_branch(&repo.to_string_lossy(), "tasque-sync", "test")
            .expect_err("expected error");
        assert_eq!(err.code, "GIT_NOT_AVAILABLE");
    }

    #[test]
    fn setup_sync_branch_fails_on_invalid_branch_name() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = dir.path();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .output()
            .expect("git init");

        std::fs::create_dir_all(repo.join(".tasque")).expect("mkdir");
        let config = crate::types::Config {
            schema_version: 1,
            snapshot_every: 200,
            sync_branch: None,
        };
        write_config(repo, &config).expect("write_config");

        let err = setup_sync_branch(&repo.to_string_lossy(), "bad branch name", "test")
            .expect_err("expected error");
        assert_eq!(err.code, "INVALID_BRANCH_NAME");
    }

    #[test]
    fn ensure_seed_tasque_dir_creates_required_files() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let tasque = dir.path().join(".tasque");

        ensure_seed_tasque_dir(&tasque).expect("ensure_seed");

        assert!(tasque.join("events.jsonl").exists());
        assert!(tasque.join("config.json").exists());
        assert!(tasque.join(".gitignore").exists());
    }

    #[test]
    fn ensure_seed_tasque_dir_is_idempotent() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let tasque = dir.path().join(".tasque");

        ensure_seed_tasque_dir(&tasque).expect("first call");
        std::fs::write(tasque.join("events.jsonl"), "existing\n").expect("write");

        ensure_seed_tasque_dir(&tasque).expect("second call");

        let content = std::fs::read_to_string(tasque.join("events.jsonl")).expect("read");
        assert_eq!(content, "existing\n");
    }

    #[test]
    fn migrate_does_not_duplicate_events_when_branch_is_new() {
        unsafe {
            std::env::set_var("GIT_AUTHOR_NAME", "tasque-test");
            std::env::set_var("GIT_AUTHOR_EMAIL", "tasque@example.com");
            std::env::set_var("GIT_COMMITTER_NAME", "tasque-test");
            std::env::set_var("GIT_COMMITTER_EMAIL", "tasque@example.com");
        }

        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = dir.path();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .output()
            .expect("git init");

        std::fs::create_dir_all(repo.join(".tasque")).expect("mkdir");
        let config = crate::types::Config {
            schema_version: 1,
            snapshot_every: 200,
            sync_branch: None,
        };
        write_config(repo, &config).expect("write_config");

        let mut created_payload = serde_json::Map::new();
        created_payload.insert(
            "title".to_string(),
            serde_json::Value::String("Example".to_string()),
        );
        let events = vec![
            crate::types::EventRecord {
                id: Some("01AAA".to_string()),
                event_id: Some("01AAA".to_string()),
                ts: "2026-01-01T00:00:00Z".to_string(),
                actor: "test".to_string(),
                event_type: crate::types::EventType::TaskCreated,
                task_id: "tsq-01AAA".to_string(),
                payload: created_payload,
            },
            crate::types::EventRecord {
                id: Some("01AAB".to_string()),
                event_id: Some("01AAB".to_string()),
                ts: "2026-01-01T00:00:01Z".to_string(),
                actor: "test".to_string(),
                event_type: crate::types::EventType::TaskUpdated,
                task_id: "tsq-01AAA".to_string(),
                payload: serde_json::Map::new(),
            },
        ];
        append_events(repo, &events).expect("append_events");

        let result = migrate_to_sync_branch(&repo.to_string_lossy(), "tasque-sync", "test")
            .expect("migrate");
        assert_eq!(result.events_migrated, 0);

        let migrated = read_events(&result.worktree_path).expect("read_events");
        assert_eq!(migrated.events.len(), 2);

        let root_events = read_events(repo).expect("read_events");
        assert_eq!(root_events.events.len(), 0);
    }
}
