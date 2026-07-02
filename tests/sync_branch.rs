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

/// Build a legacy repo: main-tree `.tasque` data (no `sync_branch` in
/// config), then git-init it and point `origin` at a nonexistent path so
/// any push fails.
fn make_legacy_repo_with_unreachable_origin(
    base: &std::path::Path,
    title: &str,
) -> std::path::PathBuf {
    let root = base.join("repo");
    fs::create_dir(&root).expect("repo dir");

    let init = run_cli(&root, ["init"]);
    assert_eq!(init.code, 0, "stderr: {}", init.stderr);
    let create = run_cli(&root, ["create", title]);
    assert_eq!(create.code, 0, "stderr: {}", create.stderr);

    let config = fs::read_to_string(root.join(".tasque").join("config.json")).expect("config");
    assert!(
        !config.contains("\"sync_branch\": \""),
        "expected legacy config without sync_branch:\n{}",
        config
    );

    init_git_repo_with_identity(&root, Some("main"));
    let missing_remote = base.join("missing-origin.git");
    let missing_remote_arg = missing_remote.to_string_lossy().to_string();
    git(
        &root,
        &["remote", "add", "origin", missing_remote_arg.as_str()],
    );

    root
}

/// Implicit migration (any data command in a legacy repo) must not hard-fail
/// a read path when the push to origin fails: push is best-effort with a
/// stderr warning, and the main-tree events are cleared once the local
/// worktree commit succeeds.
#[test]
fn implicit_migration_survives_unreachable_origin_with_warning() {
    let repo = make_repo();
    let base = repo.path();
    let root = make_legacy_repo_with_unreachable_origin(base, "Offline implicit task");

    let list = run_cli(&root, ["find", "open", "--json"]);
    assert_eq!(
        list.code, 0,
        "expected read command to succeed despite failed push\nstdout:\n{}\nstderr:\n{}",
        list.stdout, list.stderr
    );
    let envelope: Value = serde_json::from_str(list.stdout.trim()).expect("json envelope");
    assert_eq!(envelope.get("ok").and_then(Value::as_bool), Some(true));
    assert!(
        list.stdout.contains("Offline implicit task"),
        "expected migrated task in output:\n{}",
        list.stdout
    );
    assert!(
        list.stderr
            .contains("tsq: warning: migrated events to sync branch but push to 'origin' failed"),
        "expected best-effort push warning on stderr:\n{}",
        list.stderr
    );

    let root_events =
        fs::read_to_string(root.join(".tasque").join("events.jsonl")).expect("root events");
    assert!(
        root_events.is_empty(),
        "expected main-tree events cleared after local migration"
    );
    let sync_events = fs::read_to_string(
        root.join(".git")
            .join("tsq-sync")
            .join(".tasque")
            .join("events.jsonl"),
    )
    .expect("sync worktree events");
    assert!(
        !sync_events.is_empty(),
        "expected events present in sync worktree"
    );
}

/// Explicit `tsq migrate` keeps push failures fatal, but the events must
/// already be migrated and the main tree cleared (local commit lands before
/// the push), so a later `tsq sync` can complete the push.
#[test]
fn explicit_migrate_fails_on_unreachable_origin_after_local_migration() {
    let repo = make_repo();
    let base = repo.path();
    let root = make_legacy_repo_with_unreachable_origin(base, "Offline explicit task");

    let migrate = run_cli(&root, ["migrate", "--json"]);
    assert_ne!(
        migrate.code, 0,
        "expected explicit migrate to fail when push fails\nstdout:\n{}\nstderr:\n{}",
        migrate.stdout, migrate.stderr
    );
    let envelope: Value = serde_json::from_str(migrate.stdout.trim()).expect("json envelope");
    assert_eq!(envelope.get("ok").and_then(Value::as_bool), Some(false));

    let root_events =
        fs::read_to_string(root.join(".tasque").join("events.jsonl")).expect("root events");
    assert!(
        root_events.is_empty(),
        "expected main-tree events cleared even when push fails"
    );
    let sync_events = fs::read_to_string(
        root.join(".git")
            .join("tsq-sync")
            .join(".tasque")
            .join("events.jsonl"),
    )
    .expect("sync worktree events");
    assert!(
        !sync_events.is_empty(),
        "expected events migrated locally despite failed push"
    );
}
