mod common;

use common::{git, git_output, init_git_repo_with_identity, make_repo, run_cli};
use serde_json::Value;
use std::fs;

fn git_out(repo: &std::path::Path, args: &[&str]) -> String {
    let output = git_output(repo, args);
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn create_bare_origin(base: &std::path::Path) -> std::path::PathBuf {
    let remote = base.join("origin.git");
    let remote_arg = remote.to_string_lossy().to_string();
    git(base, &["init", "--bare", remote_arg.as_str()]);
    git(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    remote
}

#[test]
fn sync_branch_requires_git_repo() {
    let repo = make_repo();
    let root = repo.path();
    fs::create_dir_all(root.join(".tasque")).unwrap();
    let config = serde_json::json!({
        "schema_version": 1,
        "snapshot_every": 200,
        "sync_branch": "tasque-sync"
    });
    fs::write(
        root.join(".tasque").join("config.json"),
        format!("{}\n", config),
    )
    .unwrap();

    let result = run_cli(root, ["find", "open", "--json"]);
    assert_eq!(result.code, 2);
    let envelope: Value = serde_json::from_str(result.stdout.trim()).unwrap();
    let code = envelope
        .get("error")
        .and_then(|value| value.get("code"))
        .and_then(Value::as_str);
    assert_eq!(code, Some("GIT_NOT_AVAILABLE"));
}

const EXPECTED_MERGE_DRIVER: &str = "tsq merge-driver %O %A %B";

/// Fresh clones only receive the sync branch and its committed
/// `.gitattributes` (`merge=tasque-events`) via git; the merge driver
/// *definition* lives in local git config and does not travel with a clone.
/// Materializing the sync worktree on a clone (via any data command) must
/// configure the driver so `git merge`/`git pull` on a diverged sync branch
/// invokes `tsq merge-driver` instead of falling back to a default textual
/// 3-way merge on JSONL.
#[test]
fn clone_materializes_worktree_and_configures_merge_driver() {
    let repo = make_repo();
    let base = repo.path();
    let source = base.join("source");
    fs::create_dir(&source).expect("source dir");

    init_git_repo_with_identity(&source, Some("main"));

    let init = run_cli(&source, ["init"]);
    assert_eq!(init.code, 0, "stderr: {}", init.stderr);
    let create = run_cli(&source, ["create", "Driver clone task"]);
    assert_eq!(create.code, 0, "stderr: {}", create.stderr);

    git(&source, &["add", ".tasque/config.json", ".gitattributes"]);
    git(&source, &["commit", "-m", "seed main config"]);

    let remote = create_bare_origin(base);
    let remote_arg = remote.to_string_lossy().to_string();
    git(&source, &["remote", "add", "origin", remote_arg.as_str()]);
    git(&source, &["push", "origin", "HEAD:main"]);
    git(&source, &["push", "origin", "tsq-sync"]);

    let clone = base.join("clone");
    let clone_arg = clone.to_string_lossy().to_string();
    git(base, &["clone", remote_arg.as_str(), clone_arg.as_str()]);

    // Sanity: a fresh clone has no local merge driver config at all.
    let pre_check = git_output(&clone, &["config", "--get", "merge.tasque-events.driver"]);
    assert!(
        !pre_check.status.success(),
        "expected no merge driver config before the worktree materializes"
    );

    // First data command in the clone materializes the sync worktree
    // (cold `ensure_worktree` path in `resolve_effective_root`).
    let list = run_cli(&clone, ["find", "open", "--json"]);
    assert_eq!(list.code, 0, "stderr: {}", list.stderr);
    assert!(
        list.stdout.contains("Driver clone task"),
        "expected cloned repo to read remote sync task:\n{}",
        list.stdout
    );

    let driver = git_out(&clone, &["config", "--get", "merge.tasque-events.driver"]);
    assert_eq!(
        driver, EXPECTED_MERGE_DRIVER,
        "expected merge driver to be configured after worktree materializes"
    );

    // `tsq sync` heals a clone whose worktree was materialized on a
    // pre-fix binary: manually unset the driver, then verify `tsq sync`
    // restores it.
    git(&clone, &["config", "--unset", "merge.tasque-events.driver"]);
    let missing = git_output(&clone, &["config", "--get", "merge.tasque-events.driver"]);
    assert!(
        !missing.status.success(),
        "expected merge driver config to be unset before sync"
    );

    let sync_result = run_cli(&clone, ["sync", "--no-push"]);
    assert_eq!(sync_result.code, 0, "stderr: {}", sync_result.stderr);

    let restored = git_out(&clone, &["config", "--get", "merge.tasque-events.driver"]);
    assert_eq!(
        restored, EXPECTED_MERGE_DRIVER,
        "expected `tsq sync` to re-ensure the merge driver config"
    );
}
