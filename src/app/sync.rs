use crate::errors::TsqError;
use crate::store::config::{read_config, write_config};
use crate::store::events::{append_events, read_events};
use crate::store::git;
use crate::store::paths::get_paths;
use crate::types::{
    HookInstallResult, HookUninstallResult, MigrateResult, SyncRunResult, SyncSetupResult,
};
use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

const SYNC_COMMIT_MESSAGE: &str = "chore(tsq): sync task updates";
const HOOK_MARKER: &str = "tsq-sync-pre-push-hook";
const SETUP_LOCK_TIMEOUT_MS: u64 = 120_000;

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
    if let Some(quick_path) = git::quick_worktree_path(repo_path)
        && git::worktree_is_valid(&quick_path, &branch)
    {
        return Ok(quick_path.to_string_lossy().to_string());
    }

    if !git::is_git_repo(repo_path) {
        return Err(TsqError::new(
            "GIT_NOT_AVAILABLE",
            "sync_branch is configured but repo is not a git repository",
            2,
        ));
    }

    let worktree = with_setup_lock(repo_root, || git::ensure_worktree(repo_path, &branch))?;
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
    with_setup_lock(repo_root, || setup_sync_branch_locked(repo_root, branch))
}

fn setup_sync_branch_locked(repo_root: &str, branch: &str) -> Result<SyncSetupResult, TsqError> {
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
    let _ = git::commit_worktree(wt_path, "chore: migrate tasque events to sync branch")?;
    clear_repo_events(repo_root)?;

    Ok(MigrateResult {
        events_migrated: to_append.len(),
        branch: setup.branch,
        worktree_path: setup.worktree_path,
    })
}

pub fn sync_worktree(repo_root: &str, push: bool) -> Result<SyncRunResult, TsqError> {
    let path = Path::new(repo_root);
    if !git::is_git_repo(path) {
        return Err(TsqError::new(
            "GIT_NOT_AVAILABLE",
            "sync requires a git repository",
            2,
        ));
    }
    if !git::is_sync_worktree_path(path) {
        return Err(TsqError::new(
            "SYNC_NOT_CONFIGURED",
            "sync branch is not configured for this repository",
            1,
        ));
    }

    let branch = git::current_branch(path)?
        .ok_or_else(|| TsqError::new("GIT_ERROR", "failed determining current branch", 2))?;
    let committed = git::commit_worktree(path, SYNC_COMMIT_MESSAGE)?;
    let has_upstream = git::has_upstream(path)?;
    let pushed = if push && has_upstream {
        git::push_current(path)?;
        true
    } else {
        false
    };
    Ok(SyncRunResult {
        branch,
        worktree_path: path.to_string_lossy().to_string(),
        committed,
        pushed,
        has_upstream,
    })
}

pub fn auto_commit_if_sync_worktree(repo_root: impl AsRef<Path>) -> Result<(), TsqError> {
    let path = repo_root.as_ref();
    if !git::is_sync_worktree_path(path) {
        return Ok(());
    }
    let _ = git::commit_worktree(path, SYNC_COMMIT_MESSAGE)?;
    Ok(())
}

pub fn install_hooks(repo_root: &str, force: bool) -> Result<HookInstallResult, TsqError> {
    with_setup_lock(repo_root, || install_hooks_locked(repo_root, force))
}

fn install_hooks_locked(repo_root: &str, force: bool) -> Result<HookInstallResult, TsqError> {
    let repo = Path::new(repo_root);
    if !git::is_git_repo(repo) {
        return Err(TsqError::new(
            "GIT_NOT_AVAILABLE",
            "hook installation requires a git repository",
            2,
        ));
    }

    let hooks_dir = git::hooks_dir(repo)?;
    std::fs::create_dir_all(&hooks_dir).map_err(|error| {
        TsqError::new("HOOK_INSTALL_FAILED", "failed creating hooks directory", 2)
            .with_details(serde_json::json!({"message": error.to_string()}))
    })?;
    let pre_push = hooks_dir.join("pre-push");
    let hook_path = pre_push.to_string_lossy().to_string();
    let desired = hook_script();

    if pre_push.exists() {
        let existing = std::fs::read_to_string(&pre_push).map_err(|error| {
            TsqError::new(
                "HOOK_INSTALL_FAILED",
                "failed reading existing pre-push hook",
                2,
            )
            .with_details(serde_json::json!({"message": error.to_string()}))
        })?;
        if existing.contains(HOOK_MARKER) {
            return Ok(HookInstallResult {
                hook_path,
                installed: true,
            });
        }
        if !force {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "pre-push hook already exists; rerun with --force to overwrite",
                1,
            )
            .with_details(serde_json::json!({ "path": pre_push.display().to_string() })));
        }
    }

    std::fs::write(&pre_push, desired).map_err(|error| {
        TsqError::new("HOOK_INSTALL_FAILED", "failed writing pre-push hook", 2)
            .with_details(serde_json::json!({"message": error.to_string()}))
    })?;
    set_hook_permissions(&pre_push)?;
    Ok(HookInstallResult {
        hook_path,
        installed: true,
    })
}

pub fn uninstall_hooks(repo_root: &str) -> Result<HookUninstallResult, TsqError> {
    with_setup_lock(repo_root, || uninstall_hooks_locked(repo_root))
}

fn uninstall_hooks_locked(repo_root: &str) -> Result<HookUninstallResult, TsqError> {
    let repo = Path::new(repo_root);
    if !git::is_git_repo(repo) {
        return Err(TsqError::new(
            "GIT_NOT_AVAILABLE",
            "hook uninstall requires a git repository",
            2,
        ));
    }

    let pre_push = git::hooks_dir(repo)?.join("pre-push");
    let hook_path = pre_push.to_string_lossy().to_string();
    if !pre_push.exists() {
        return Ok(HookUninstallResult {
            hook_path,
            removed: false,
        });
    }

    let content = std::fs::read_to_string(&pre_push).map_err(|error| {
        TsqError::new("HOOK_UNINSTALL_FAILED", "failed reading pre-push hook", 2)
            .with_details(serde_json::json!({"message": error.to_string()}))
    })?;
    if !content.contains(HOOK_MARKER) {
        return Ok(HookUninstallResult {
            hook_path,
            removed: false,
        });
    }

    std::fs::remove_file(&pre_push).map_err(|error| {
        TsqError::new("HOOK_UNINSTALL_FAILED", "failed removing pre-push hook", 2)
            .with_details(serde_json::json!({"message": error.to_string()}))
    })?;
    Ok(HookUninstallResult {
        hook_path,
        removed: true,
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

fn hook_script() -> String {
    format!("#!/bin/sh\n# {}\nset -e\ntsq sync --no-push\n", HOOK_MARKER)
}

fn with_setup_lock<T, F>(repo_root: &str, f: F) -> Result<T, TsqError>
where
    F: FnOnce() -> Result<T, TsqError>,
{
    let paths = get_paths(repo_root);
    std::fs::create_dir_all(&paths.tasque_dir).map_err(|error| {
        TsqError::new("SYNC_SETUP_FAILED", "failed creating .tasque directory", 2)
            .with_details(serde_json::json!({"message": error.to_string()}))
    })?;
    let lock_file = paths.tasque_dir.join(".setup.lock");
    let deadline = Instant::now() + Duration::from_millis(SETUP_LOCK_TIMEOUT_MS);

    loop {
        let opened = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_file);
        match opened {
            Ok(_) => break,
            Err(error) => {
                if error.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(TsqError::new(
                        "SYNC_SETUP_FAILED",
                        "failed acquiring setup lock",
                        2,
                    )
                    .with_details(serde_json::json!({"message": error.to_string()})));
                }
                if Instant::now() >= deadline {
                    return Err(TsqError::new(
                        "SYNC_SETUP_TIMEOUT",
                        "timed out waiting for setup lock",
                        3,
                    )
                    .with_details(
                        serde_json::json!({"lockFile": lock_file.display().to_string()}),
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    let result = f();
    let _ = std::fs::remove_file(&lock_file);
    result
}

#[cfg(unix)]
fn set_hook_permissions(path: &Path) -> Result<(), TsqError> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(path, permissions).map_err(|error| {
        TsqError::new(
            "HOOK_INSTALL_FAILED",
            "failed setting pre-push hook permissions",
            2,
        )
        .with_details(serde_json::json!({"message": error.to_string()}))
    })
}

#[cfg(not(unix))]
fn set_hook_permissions(_path: &Path) -> Result<(), TsqError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

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

    #[test]
    fn setup_sync_branch_is_safe_under_parallel_calls() {
        unsafe {
            std::env::set_var("GIT_AUTHOR_NAME", "tasque-test");
            std::env::set_var("GIT_AUTHOR_EMAIL", "tasque@example.com");
            std::env::set_var("GIT_COMMITTER_NAME", "tasque-test");
            std::env::set_var("GIT_COMMITTER_EMAIL", "tasque@example.com");
        }

        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = dir.path().to_path_buf();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .expect("git init");
        std::process::Command::new("git")
            .args(["config", "user.name", "tasque-test"])
            .current_dir(&repo)
            .output()
            .expect("git config");
        std::process::Command::new("git")
            .args(["config", "user.email", "tasque@example.com"])
            .current_dir(&repo)
            .output()
            .expect("git config");
        std::fs::write(repo.join("README.md"), "seed\n").expect("write");
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .expect("git add");
        std::process::Command::new("git")
            .args(["commit", "-m", "seed"])
            .current_dir(&repo)
            .output()
            .expect("git commit");

        std::fs::create_dir_all(repo.join(".tasque")).expect("mkdir");
        let config = crate::types::Config {
            schema_version: 1,
            snapshot_every: 200,
            sync_branch: None,
        };
        write_config(&repo, &config).expect("write_config");

        let repo1 = Arc::new(repo);
        let repo2 = repo1.clone();
        let t1 = std::thread::spawn(move || {
            setup_sync_branch(&repo1.to_string_lossy(), "tasque-sync", "test")
        });
        let t2 = std::thread::spawn(move || {
            setup_sync_branch(&repo2.to_string_lossy(), "tasque-sync", "test")
        });

        let r1 = t1.join().expect("thread1");
        let r2 = t2.join().expect("thread2");
        assert!(r1.is_ok(), "thread1 failed: {:?}", r1.err());
        assert!(r2.is_ok(), "thread2 failed: {:?}", r2.err());
    }
}
