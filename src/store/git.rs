use crate::errors::TsqError;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn git_not_available() -> TsqError {
    TsqError::new(
        "GIT_NOT_AVAILABLE",
        "git is not available on this system",
        2,
    )
}

fn git_error(message: impl Into<String>, stderr: impl Into<String>) -> TsqError {
    TsqError::new("GIT_ERROR", message.into(), 2)
        .with_details(serde_json::json!({ "stderr": stderr.into() }))
}

fn run_git(repo: &Path, args: &[&str]) -> Result<String, TsqError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|_| git_not_available())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(git_error(
            format!("git {} failed", args.first().unwrap_or(&"<unknown>")),
            stderr,
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git_status(repo: &Path, args: &[&str]) -> Result<bool, TsqError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|_| git_not_available())?;

    Ok(output.status.success())
}

// ---------------------------------------------------------------------------
// Branch name validation
// ---------------------------------------------------------------------------

/// Validate a git branch name. Allows only `[a-zA-Z0-9_/-]`, rejects empty,
/// names starting with `-`, names containing `..` or spaces.
pub fn validate_branch_name(name: &str) -> Result<(), TsqError> {
    if name.is_empty() {
        return Err(TsqError::new(
            "INVALID_BRANCH_NAME",
            "Branch name must not be empty",
            1,
        ));
    }
    if name.starts_with('-') {
        return Err(TsqError::new(
            "INVALID_BRANCH_NAME",
            "Branch name must not start with '-'",
            1,
        ));
    }
    if name.contains("..") {
        return Err(TsqError::new(
            "INVALID_BRANCH_NAME",
            "Branch name must not contain '..'",
            1,
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '/')
    {
        return Err(TsqError::new(
            "INVALID_BRANCH_NAME",
            "Branch name may only contain [a-zA-Z0-9_/-]",
            1,
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Public git operations
// ---------------------------------------------------------------------------

/// Returns the `.git` directory path for the repo.
pub fn git_dir(repo_root: &Path) -> Result<PathBuf, TsqError> {
    let out = run_git(repo_root, &["rev-parse", "--git-dir"])?;
    let p = Path::new(&out);
    if p.is_absolute() {
        Ok(p.to_path_buf())
    } else {
        Ok(repo_root.join(p))
    }
}

/// Returns the current branch name, or `None` if in detached HEAD state.
pub fn current_branch(repo_root: &Path) -> Result<Option<String>, TsqError> {
    let out = run_git(repo_root, &["branch", "--show-current"])?;
    if out.is_empty() {
        Ok(None)
    } else {
        Ok(Some(out))
    }
}

pub fn current_upstream_remote(repo_root: &Path) -> Result<Option<String>, TsqError> {
    let Some(branch) = current_branch(repo_root)? else {
        return Ok(None);
    };
    let key = format!("branch.{branch}.remote");
    if run_git_status(repo_root, &["config", "--get", &key])? {
        let remote = run_git(repo_root, &["config", "--get", &key])?;
        if !remote.is_empty() && remote != "." {
            return Ok(Some(remote));
        }
    }
    if has_remote(repo_root, "origin")? {
        return Ok(Some("origin".to_string()));
    }
    Ok(None)
}

/// Returns true if a local branch with the given name exists.
pub fn branch_exists(repo_root: &Path, name: &str) -> Result<bool, TsqError> {
    validate_branch_name(name)?;
    let refspec = format!("refs/heads/{}", name);
    run_git_status(repo_root, &["show-ref", "--verify", "--quiet", &refspec])
}

pub fn remote_branch_exists(repo_root: &Path, name: &str) -> Result<bool, TsqError> {
    validate_branch_name(name)?;
    let refspec = format!("refs/heads/{}", name);
    run_git_status(
        repo_root,
        &["ls-remote", "--exit-code", "--heads", "origin", &refspec],
    )
}

pub fn remote_tracking_branch_exists(repo_root: &Path, name: &str) -> Result<bool, TsqError> {
    validate_branch_name(name)?;
    let refspec = format!("refs/remotes/origin/{}", name);
    run_git_status(repo_root, &["show-ref", "--verify", "--quiet", &refspec])
}

pub fn track_remote_branch(repo_root: &Path, name: &str) -> Result<(), TsqError> {
    validate_branch_name(name)?;
    run_git(
        repo_root,
        &["branch", "--track", name, &format!("origin/{name}")],
    )?;
    Ok(())
}

pub fn fetch_remote_branch(repo_root: &Path, name: &str) -> Result<(), TsqError> {
    validate_branch_name(name)?;
    let remote_ref = format!("+refs/heads/{name}:refs/remotes/origin/{name}");
    run_git(repo_root, &["fetch", "origin", &remote_ref])?;
    track_remote_branch(repo_root, name)?;
    Ok(())
}

pub fn has_upstream(repo_root: &Path) -> Result<bool, TsqError> {
    run_git_status(
        repo_root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
}

pub fn has_remote(repo_root: &Path, name: &str) -> Result<bool, TsqError> {
    run_git_status(repo_root, &["remote", "get-url", name])
}

pub fn push_current(repo_root: &Path) -> Result<(), TsqError> {
    run_git(repo_root, &["push"])?;
    Ok(())
}

pub fn push_current_set_upstream(
    repo_root: &Path,
    remote: &str,
    branch: &str,
) -> Result<(), TsqError> {
    validate_branch_name(branch)?;
    run_git(repo_root, &["push", "-u", remote, branch])?;
    Ok(())
}

/// Returns true if the path is inside a git working tree.
pub fn is_git_repo(repo_root: &Path) -> bool {
    run_git_status(repo_root, &["rev-parse", "--is-inside-work-tree"]).unwrap_or(false)
}

pub fn git_common_dir(repo_root: &Path) -> Result<PathBuf, TsqError> {
    if let Some(path) = fast_git_common_dir(repo_root) {
        return Ok(path);
    }
    let out = run_git(repo_root, &["rev-parse", "--git-common-dir"])?;
    let path = Path::new(&out);
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(repo_root.join(path))
    }
}

pub fn hooks_dir(repo_root: &Path) -> Result<PathBuf, TsqError> {
    Ok(git_common_dir(repo_root)?.join("hooks"))
}

pub fn is_sync_worktree_path(repo_root: &Path) -> bool {
    if !repo_root.join(".tasque").join("events.jsonl").exists() {
        return false;
    }

    if let Some(common_dir) = fast_git_common_dir(repo_root)
        && let (Ok(root), Ok(common)) = (repo_root.canonicalize(), common_dir.canonicalize())
        && root.starts_with(common)
    {
        return true;
    }

    if !repo_root.join(".git").is_file() {
        return false;
    }

    repo_root
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value == "tsq-sync" || value == "tasque-sync-worktree")
        .unwrap_or(false)
}

pub fn quick_worktree_path(repo_root: &Path, branch: &str) -> Option<PathBuf> {
    fast_git_common_dir(repo_root).map(|path| path.join(branch))
}

pub fn existing_worktree_for_branch(
    repo_root: &Path,
    branch: &str,
) -> Result<Option<PathBuf>, TsqError> {
    validate_branch_name(branch)?;
    let expected_branch = format!("branch refs/heads/{branch}");
    let out = run_git(repo_root, &["worktree", "list", "--porcelain"])?;
    let mut current_path: Option<PathBuf> = None;

    for line in out.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(path));
            continue;
        }

        if line == expected_branch
            && let Some(path) = current_path.take()
            && worktree_is_valid(&path, branch)
        {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

pub fn worktree_is_valid(worktree_path: &Path, expected_branch: &str) -> bool {
    if !worktree_path.join(".git").exists() {
        return false;
    }
    if !worktree_path.join(".tasque").join("events.jsonl").exists() {
        return false;
    }
    read_worktree_branch(worktree_path)
        .map(|branch| branch == expected_branch)
        .unwrap_or(false)
}

/// Derive the worktree path for the sync branch.
/// Located at `<git_dir>/<branch>`.
pub fn worktree_path(repo_root: &Path, branch: &str) -> Result<PathBuf, TsqError> {
    let gd = git_common_dir(repo_root)?;
    Ok(gd.join(branch))
}

/// Ensure a git worktree exists for the given branch.
/// Creates it with sparse checkout (only `.tasque/` and `.gitattributes`).
/// Returns the worktree path.
pub fn ensure_worktree(repo_root: &Path, branch: &str) -> Result<PathBuf, TsqError> {
    validate_branch_name(branch)?;
    if let Some(wt) = existing_worktree_for_branch(repo_root, branch)? {
        return Ok(wt);
    }

    if !branch_exists(repo_root, branch)? {
        if remote_tracking_branch_exists(repo_root, branch)? {
            track_remote_branch(repo_root, branch)?;
        } else if remote_branch_exists(repo_root, branch)? {
            fetch_remote_branch(repo_root, branch)?;
        }
    }
    let wt = worktree_path(repo_root, branch)?;

    if wt.exists() {
        if worktree_is_valid(&wt, branch) {
            return Ok(wt);
        }
        let wt_str = wt.to_string_lossy().to_string();
        let _ = run_git_status(repo_root, &["worktree", "remove", "--force", &wt_str]);
        if wt.exists() {
            std::fs::remove_dir_all(&wt).map_err(|e| {
                git_error(
                    format!("Failed removing stale worktree at {}", wt.display()),
                    e.to_string(),
                )
            })?;
        }
    }

    // Create the worktree
    run_git(
        repo_root,
        &["worktree", "add", &wt.to_string_lossy(), branch],
    )?;

    configure_sparse_checkout(&wt)?;

    Ok(wt)
}

fn configure_sparse_checkout(wt: &Path) -> Result<(), TsqError> {
    run_git(wt, &["sparse-checkout", "init", "--no-cone"])?;
    run_git(
        wt,
        &["sparse-checkout", "set", "/.tasque", "/.gitattributes"],
    )?;
    Ok(())
}

fn fast_git_common_dir(repo_root: &Path) -> Option<PathBuf> {
    let dot_git = repo_root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    if !dot_git.is_file() {
        return None;
    }
    let raw = std::fs::read_to_string(&dot_git).ok()?;
    let raw = raw.trim();
    let prefix = "gitdir:";
    if !raw.starts_with(prefix) {
        return None;
    }
    let gitdir_raw = raw[prefix.len()..].trim();
    let gitdir_path = if Path::new(gitdir_raw).is_absolute() {
        PathBuf::from(gitdir_raw)
    } else {
        repo_root.join(gitdir_raw)
    };
    let common_dir_file = gitdir_path.join("commondir");
    if !common_dir_file.exists() {
        return Some(gitdir_path);
    }
    let common_raw = std::fs::read_to_string(common_dir_file).ok()?;
    let common_raw = common_raw.trim();
    if common_raw.is_empty() {
        return Some(gitdir_path);
    }
    let common_path = if Path::new(common_raw).is_absolute() {
        PathBuf::from(common_raw)
    } else {
        gitdir_path.join(common_raw)
    };
    Some(common_path)
}

fn read_worktree_branch(worktree_path: &Path) -> Option<String> {
    let dot_git_path = worktree_path.join(".git");
    let gitdir = if dot_git_path.is_file() {
        let raw = std::fs::read_to_string(&dot_git_path).ok()?;
        let raw = raw.trim();
        let prefix = "gitdir:";
        if !raw.starts_with(prefix) {
            return None;
        }
        let value = raw[prefix.len()..].trim();
        let parsed = Path::new(value);
        if parsed.is_absolute() {
            parsed.to_path_buf()
        } else {
            worktree_path.join(parsed)
        }
    } else if dot_git_path.is_dir() {
        dot_git_path
    } else {
        return None;
    };
    let head = std::fs::read_to_string(gitdir.join("HEAD")).ok()?;
    let head = head.trim();
    let ref_prefix = "ref: refs/heads/";
    head.strip_prefix(ref_prefix).map(|value| value.to_string())
}

/// Create an orphan branch containing only `.tasque/` and `.gitattributes`.
///
/// Uses a temporary git repo to avoid touching the user's index/working tree.
/// Steps:
///   1. Create temp dir and `git init`
///   2. Copy `.tasque/` contents from `tasque_dir_contents`
///   3. Create `.gitattributes` with merge driver entry
///   4. `git add . && git commit`
///   5. From `repo_root`: `git fetch <tmp> <branch>:<branch>`
///   6. Clean up temp dir
pub fn create_orphan_branch(
    repo_root: &Path,
    branch: &str,
    tasque_dir_contents: &Path,
) -> Result<(), TsqError> {
    validate_branch_name(branch)?;

    let tmp = tempfile::tempdir()
        .map_err(|e| git_error("Failed creating temp dir for orphan branch", e.to_string()))?;
    let tmp_path = tmp.path();

    // Init temp repo
    run_git(tmp_path, &["init"])?;
    run_git(tmp_path, &["config", "user.name", "tsq"])?;
    run_git(tmp_path, &["config", "user.email", "tsq@example.invalid"])?;

    // Copy .tasque/ contents
    let dst_tasque = tmp_path.join(".tasque");
    copy_dir_recursive(tasque_dir_contents, &dst_tasque)?;

    // Create .gitattributes
    let gitattributes = tmp_path.join(".gitattributes");
    std::fs::write(&gitattributes, ".tasque/events.jsonl merge=tasque-events\n")
        .map_err(|e| git_error("Failed writing .gitattributes", e.to_string()))?;

    // Stage and commit
    run_git(tmp_path, &["add", "."])?;
    run_git(tmp_path, &["commit", "-m", "Initial tasque sync branch"])?;

    // Rename the default branch to match the desired name
    run_git(tmp_path, &["branch", "-M", branch])?;

    // Fetch from temp into the real repo
    let tmp_str = tmp_path.to_string_lossy();
    let refspec = format!("{}:{}", branch, branch);
    run_git(repo_root, &["fetch", &tmp_str, &refspec])?;

    // tmp is automatically cleaned up by Drop
    Ok(())
}

/// Stage all changes in a worktree and commit with the given message.
/// Returns true when a commit was created.
pub fn commit_worktree(wt_path: &Path, message: &str) -> Result<bool, TsqError> {
    run_git(wt_path, &["add", "."])?;

    // Check if there are staged changes
    let has_changes = !run_git_status(wt_path, &["diff", "--cached", "--quiet"])?;
    if !has_changes {
        return Ok(false); // Nothing to commit
    }

    run_git(wt_path, &["commit", "-m", message])?;
    Ok(true)
}

pub fn ensure_gitattributes_entry(repo_root: &Path) -> Result<bool, TsqError> {
    let path = repo_root.join(".gitattributes");
    let line = ".tasque/events.jsonl merge=tasque-events";
    let existing = match std::fs::read_to_string(&path) {
        Ok(value) => value,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                String::new()
            } else {
                return Err(git_error(
                    "Failed reading .gitattributes",
                    error.to_string(),
                ));
            }
        }
    };

    if existing.lines().any(|value| value.trim() == line) {
        return Ok(false);
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(line);
    updated.push('\n');
    std::fs::write(&path, updated)
        .map_err(|error| git_error("Failed writing .gitattributes", error.to_string()))?;
    Ok(true)
}

/// Configure the tasque-events merge driver in the repo's git config.
pub fn setup_merge_driver_config(repo_root: &Path) -> Result<(), TsqError> {
    run_git(
        repo_root,
        &[
            "config",
            "merge.tasque-events.name",
            "Tasque JSONL event merge",
        ],
    )?;
    run_git(
        repo_root,
        &[
            "config",
            "merge.tasque-events.driver",
            "tsq merge-driver %O %A %B",
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), TsqError> {
    std::fs::create_dir_all(dst).map_err(|e| {
        git_error(
            format!("Failed creating directory {}", dst.display()),
            e.to_string(),
        )
    })?;

    for entry in std::fs::read_dir(src).map_err(|e| {
        git_error(
            format!("Failed reading directory {}", src.display()),
            e.to_string(),
        )
    })? {
        let entry = entry.map_err(|e| git_error("Failed reading dir entry", e.to_string()))?;
        let file_name = entry.file_name();
        if should_skip_tasque_seed_entry(&file_name.to_string_lossy()) {
            continue;
        }
        let src_path = entry.path();
        let dst_path = dst.join(file_name);

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).map_err(|e| {
                git_error(
                    format!(
                        "Failed copying {} to {}",
                        src_path.display(),
                        dst_path.display()
                    ),
                    e.to_string(),
                )
            })?;
        }
    }
    Ok(())
}

fn should_skip_tasque_seed_entry(name: &str) -> bool {
    name == ".lock"
        || name == ".setup.lock"
        || name == "state.json"
        || name == "tasks.jsonl"
        || name == "snapshots"
        || name.starts_with("state.json.tmp")
        || name.ends_with(".tmp")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_branch_name_valid() {
        assert!(validate_branch_name("tasque-sync").is_ok());
        assert!(validate_branch_name("my_branch").is_ok());
        assert!(validate_branch_name("feat/sync").is_ok());
        assert!(validate_branch_name("ABC123").is_ok());
    }

    #[test]
    fn test_validate_branch_name_empty() {
        let err = validate_branch_name("").unwrap_err();
        assert_eq!(err.code, "INVALID_BRANCH_NAME");
    }

    #[test]
    fn test_validate_branch_name_starts_with_dash() {
        let err = validate_branch_name("-bad").unwrap_err();
        assert_eq!(err.code, "INVALID_BRANCH_NAME");
    }

    #[test]
    fn test_validate_branch_name_double_dot() {
        let err = validate_branch_name("a..b").unwrap_err();
        assert_eq!(err.code, "INVALID_BRANCH_NAME");
    }

    #[test]
    fn test_validate_branch_name_spaces() {
        let err = validate_branch_name("has space").unwrap_err();
        assert_eq!(err.code, "INVALID_BRANCH_NAME");
    }

    #[test]
    fn test_validate_branch_name_special_chars() {
        let err = validate_branch_name("bad@name").unwrap_err();
        assert_eq!(err.code, "INVALID_BRANCH_NAME");
    }

    #[test]
    fn test_is_git_repo_on_temp_dir() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!is_git_repo(tmp.path()));
    }

    #[test]
    fn test_is_git_repo_on_real_repo() {
        let tmp = tempfile::tempdir().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert!(is_git_repo(tmp.path()));
    }

    #[test]
    fn test_git_dir_returns_path() {
        let tmp = tempfile::tempdir().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let gd = git_dir(tmp.path()).unwrap();
        assert!(gd.exists());
        assert!(gd.ends_with(".git"));
    }

    #[test]
    fn test_current_branch_on_fresh_repo() {
        let tmp = tempfile::tempdir().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        // Fresh repo with no commits — branch --show-current may return empty or default
        let result = current_branch(tmp.path());
        // Either succeeds with a name or is Ok(None) — both are valid
        assert!(result.is_ok());
    }

    #[test]
    fn test_branch_exists_false_for_missing() {
        let tmp = tempfile::tempdir().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert!(!branch_exists(tmp.path(), "nonexistent").unwrap());
    }

    #[test]
    fn test_setup_merge_driver_config() {
        let tmp = tempfile::tempdir().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        setup_merge_driver_config(tmp.path()).unwrap();

        // Verify config was set
        let name = run_git(tmp.path(), &["config", "merge.tasque-events.name"]).unwrap();
        assert_eq!(name, "Tasque JSONL event merge");
        let driver = run_git(tmp.path(), &["config", "merge.tasque-events.driver"]).unwrap();
        assert_eq!(driver, "tsq merge-driver %O %A %B");
    }

    #[test]
    fn test_ensure_gitattributes_entry_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let updated = ensure_gitattributes_entry(tmp.path()).unwrap();
        assert!(updated);
        let content = std::fs::read_to_string(tmp.path().join(".gitattributes")).unwrap();
        assert!(content.contains(".tasque/events.jsonl merge=tasque-events"));
    }

    #[test]
    fn test_ensure_gitattributes_entry_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".gitattributes");
        std::fs::write(&path, ".tasque/events.jsonl merge=tasque-events\n").unwrap();
        let updated = ensure_gitattributes_entry(tmp.path()).unwrap();
        assert!(!updated);
        let content = std::fs::read_to_string(path).unwrap();
        assert_eq!(content, ".tasque/events.jsonl merge=tasque-events\n");
    }

    #[test]
    fn test_worktree_includes_gitattributes() {
        unsafe {
            std::env::set_var("GIT_AUTHOR_NAME", "tasque-test");
            std::env::set_var("GIT_AUTHOR_EMAIL", "tasque@example.com");
            std::env::set_var("GIT_COMMITTER_NAME", "tasque-test");
            std::env::set_var("GIT_COMMITTER_EMAIL", "tasque@example.com");
        }

        let tmp = tempfile::tempdir().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();

        let tasque_dir = tmp.path().join(".tasque");
        std::fs::create_dir_all(&tasque_dir).unwrap();
        std::fs::write(tasque_dir.join("events.jsonl"), "").unwrap();

        create_orphan_branch(tmp.path(), "tasque-sync", &tasque_dir).unwrap();
        let wt = ensure_worktree(tmp.path(), "tasque-sync").unwrap();

        let gitattributes = wt.join(".gitattributes");
        assert!(gitattributes.exists());
        let content = std::fs::read_to_string(gitattributes).unwrap();
        assert!(content.contains(".tasque/events.jsonl merge=tasque-events"));
    }

    #[test]
    fn test_fast_git_common_dir_with_git_file_and_commondir() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let meta = repo.join(".git-meta");
        let common = repo.join(".git-common");
        std::fs::create_dir_all(&meta).unwrap();
        std::fs::create_dir_all(&common).unwrap();
        std::fs::write(repo.join(".git"), "gitdir: .git-meta\n").unwrap();
        std::fs::write(meta.join("commondir"), "../.git-common\n").unwrap();

        let resolved = git_common_dir(&repo).unwrap();
        assert!(resolved.ends_with(".git-common"));
        assert_eq!(
            resolved.canonicalize().unwrap(),
            common.canonicalize().unwrap()
        );
    }

    #[test]
    fn test_ensure_worktree_repairs_wrong_branch_state() {
        unsafe {
            std::env::set_var("GIT_AUTHOR_NAME", "tasque-test");
            std::env::set_var("GIT_AUTHOR_EMAIL", "tasque@example.com");
            std::env::set_var("GIT_COMMITTER_NAME", "tasque-test");
            std::env::set_var("GIT_COMMITTER_EMAIL", "tasque@example.com");
        }

        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path();
        Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "tasque-test"])
            .current_dir(repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "tasque@example.com"])
            .current_dir(repo)
            .output()
            .unwrap();
        std::fs::create_dir_all(repo.join(".tasque")).unwrap();
        std::fs::write(repo.join(".tasque").join("events.jsonl"), "").unwrap();
        std::fs::write(
            repo.join(".gitattributes"),
            ".tasque/events.jsonl merge=tasque-events\n",
        )
        .unwrap();
        std::fs::write(repo.join("README.md"), "seed\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "seed"])
            .current_dir(repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["branch", "tasque-sync"])
            .current_dir(repo)
            .output()
            .unwrap();

        let wt = ensure_worktree(repo, "tasque-sync").unwrap();
        Command::new("git")
            .args(["checkout", "-b", "wrong-branch"])
            .current_dir(&wt)
            .output()
            .unwrap();
        assert!(!worktree_is_valid(&wt, "tasque-sync"));

        let repaired = ensure_worktree(repo, "tasque-sync").unwrap();
        assert!(worktree_is_valid(&repaired, "tasque-sync"));
    }

    #[test]
    fn test_ensure_worktree_reuses_existing_worktree_for_branch_at_legacy_path() {
        unsafe {
            std::env::set_var("GIT_AUTHOR_NAME", "tasque-test");
            std::env::set_var("GIT_AUTHOR_EMAIL", "tasque@example.com");
            std::env::set_var("GIT_COMMITTER_NAME", "tasque-test");
            std::env::set_var("GIT_COMMITTER_EMAIL", "tasque@example.com");
        }

        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path();
        Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .output()
            .unwrap();

        let tasque_dir = repo.join(".tasque");
        std::fs::create_dir_all(&tasque_dir).unwrap();
        std::fs::write(tasque_dir.join("events.jsonl"), "").unwrap();

        create_orphan_branch(repo, "tasque-sync", &tasque_dir).unwrap();
        let legacy_path = repo.join(".git").join("tasque-sync-worktree");
        Command::new("git")
            .args([
                "worktree",
                "add",
                &legacy_path.to_string_lossy(),
                "tasque-sync",
            ])
            .current_dir(repo)
            .output()
            .unwrap();
        configure_sparse_checkout(&legacy_path).unwrap();

        let wt = ensure_worktree(repo, "tasque-sync").unwrap();

        assert_eq!(
            wt.canonicalize().unwrap(),
            legacy_path.canonicalize().unwrap()
        );
        assert!(!repo.join(".git").join("tasque-sync").exists());
    }
}
