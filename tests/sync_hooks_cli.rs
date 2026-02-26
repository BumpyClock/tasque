mod common;

use common::{make_repo, run_cli};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git command failed");
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout:{}\nstderr:{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_out(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git command failed");
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout:{}\nstderr:{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn hooks_install_and_uninstall_manage_pre_push_hook() {
    let repo = make_repo();
    let root = repo.path();
    git(root, &["init"]);
    git(root, &["config", "user.name", "rust-test"]);
    git(root, &["config", "user.email", "rust-test@example.com"]);

    let init = run_cli(root, ["init"]);
    assert_eq!(init.code, 0, "stderr: {}", init.stderr);

    let install = run_cli(root, ["hooks", "install"]);
    assert_eq!(install.code, 0, "stderr: {}", install.stderr);

    let hook_path = root.join(".git").join("hooks").join("pre-push");
    let hook_content = fs::read_to_string(&hook_path).expect("pre-push exists");
    assert!(hook_content.contains("tsq-sync-pre-push-hook"));
    assert!(hook_content.contains("tsq sync --no-push"));

    let uninstall = run_cli(root, ["hooks", "uninstall"]);
    assert_eq!(uninstall.code, 0, "stderr: {}", uninstall.stderr);
    assert!(!hook_path.exists());
}

#[test]
fn sync_requires_sync_branch_configuration() {
    let repo = make_repo();
    let root = repo.path();
    git(root, &["init"]);
    git(root, &["config", "user.name", "rust-test"]);
    git(root, &["config", "user.email", "rust-test@example.com"]);

    let init = run_cli(root, ["init"]);
    assert_eq!(init.code, 0, "stderr: {}", init.stderr);

    let result = run_cli(root, ["sync", "--json"]);
    assert_eq!(result.code, 1, "stderr: {}", result.stderr);
    let envelope: Value = serde_json::from_str(result.stdout.trim()).expect("json envelope");
    let code = envelope
        .get("error")
        .and_then(|value| value.get("code"))
        .and_then(Value::as_str);
    assert_eq!(code, Some("SYNC_NOT_CONFIGURED"));
}

#[test]
fn create_auto_commits_and_sync_commits_pending_changes() {
    let repo = make_repo();
    let root = repo.path();
    git(root, &["init"]);
    git(root, &["config", "user.name", "rust-test"]);
    git(root, &["config", "user.email", "rust-test@example.com"]);

    let init = run_cli(root, ["init", "--sync-branch", "tasque-sync"]);
    assert_eq!(init.code, 0, "stderr: {}", init.stderr);

    let wt = root.join(".git").join("tasque-sync-worktree");
    let before = git_out(&wt, &["rev-list", "--count", "HEAD"])
        .parse::<u64>()
        .expect("commit count");

    let create = run_cli(root, ["create", "Sync auto commit test"]);
    assert_eq!(create.code, 0, "stderr: {}", create.stderr);

    let after = git_out(&wt, &["rev-list", "--count", "HEAD"])
        .parse::<u64>()
        .expect("commit count");
    assert!(after > before, "expected auto-commit after create");

    let spec_dir = wt.join(".tasque").join("specs").join("tsq-test");
    fs::create_dir_all(&spec_dir).expect("mkdir spec dir");
    fs::write(spec_dir.join("spec.md"), "pending sync").expect("write spec");

    let sync_result = run_cli(root, ["sync", "--no-push"]);
    assert_eq!(sync_result.code, 0, "stderr: {}", sync_result.stderr);

    let status = git_out(&wt, &["status", "--porcelain"]);
    assert!(status.is_empty(), "expected clean worktree after sync");
}
