mod common;

use common::{git, git_output, init_git_repo_with_identity, make_repo, run_cli, run_json};
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
fn hooks_install_and_uninstall_manage_pre_push_hook() {
    let repo = make_repo();
    let root = repo.path();
    init_git_repo_with_identity(root, None);

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
fn init_defaults_to_sync_branch_configuration_in_git_repo() {
    let repo = make_repo();
    let root = repo.path();
    init_git_repo_with_identity(root, None);

    let init = run_cli(root, ["init"]);
    assert_eq!(init.code, 0, "stderr: {}", init.stderr);

    let config = fs::read_to_string(root.join(".tasque").join("config.json")).expect("config");
    assert!(
        config.contains("\"sync_branch\": \"tsq-sync\""),
        "expected default sync branch config:\n{}",
        config
    );
    let wt = root.join(".git").join("tsq-sync");
    assert!(wt.join(".tasque").join("events.jsonl").exists());
    assert!(!wt.join(".tasque").join(".setup.lock").exists());

    let result = run_cli(root, ["sync", "--json"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    let envelope: Value = serde_json::from_str(result.stdout.trim()).expect("json envelope");
    assert_eq!(envelope.get("ok").and_then(Value::as_bool), Some(true));
}

#[test]
fn create_title_named_help_uses_sync_worktree() {
    let repo = make_repo();
    let root = repo.path();
    init_git_repo_with_identity(root, None);

    let init = run_cli(root, ["init", "--yes", "--no-wizard", "--json"]);
    assert_eq!(init.code, 0, "stderr: {}", init.stderr);

    let result = run_json(root, ["create", "help", "--force"]);
    assert_eq!(result.cli.code, 0, "stderr: {}", result.cli.stderr);

    let main_events = root.join(".tasque").join("events.jsonl");
    let main_log = fs::read_to_string(main_events).unwrap_or_default();
    assert_eq!(
        main_log.lines().count(),
        0,
        "task event should be written to sync worktree, not main store"
    );
    let sync_events = root
        .join(".git")
        .join("tsq-sync")
        .join(".tasque")
        .join("events.jsonl");
    let sync_log = fs::read_to_string(sync_events).expect("sync events");
    assert_eq!(sync_log.lines().count(), 1);
}

#[test]
fn init_install_skill_preserves_existing_sync_branch_config() {
    let repo = make_repo();
    let root = repo.path();
    init_git_repo_with_identity(root, None);
    fs::create_dir_all(root.join(".tasque")).expect("mkdir .tasque");
    let config_path = root.join(".tasque").join("config.json");
    let original_config = "{\n  \"schema_version\": 1,\n  \"snapshot_every\": 200,\n  \"sync_branch\": \"tasque-sync\"\n}\n";
    fs::write(&config_path, original_config).expect("write config");

    let skill_dir = root.join("skill-target");
    let skill_dir_arg = skill_dir.to_string_lossy().to_string();
    let result = run_json(
        root,
        [
            "init",
            "--install-skill",
            "--skill-targets",
            "codex",
            "--skill-dir-codex",
            &skill_dir_arg,
        ],
    );

    assert_eq!(result.cli.code, 0, "stderr: {}", result.cli.stderr);
    assert_eq!(
        fs::read_to_string(&config_path).expect("read config"),
        original_config
    );
    let data = result.envelope.get("data").expect("data");
    assert!(data.get("sync_setup").is_none());
    assert!(skill_dir.join("tasque").join("SKILL.md").exists());
}

#[test]
fn custom_worktree_name_auto_commits_and_syncs_pending_changes() {
    let repo = make_repo();
    let root = repo.path();
    init_git_repo_with_identity(root, None);

    let init = run_cli(root, ["init", "--worktree-name", "custom-sync"]);
    assert_eq!(init.code, 0, "stderr: {}", init.stderr);

    let config = fs::read_to_string(root.join(".tasque").join("config.json")).expect("config");
    assert!(
        config.contains("\"sync_branch\": \"custom-sync\""),
        "expected custom sync branch config:\n{}",
        config
    );

    let wt = root.join(".git").join("custom-sync");
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

#[test]
fn sync_without_upstream_pushes_branch_to_origin_and_sets_upstream() {
    let repo = make_repo();
    let base = repo.path();
    let root = base.join("repo");
    fs::create_dir(&root).expect("repo dir");

    init_git_repo_with_identity(&root, Some("main"));
    fs::write(root.join("README.md"), "seed\n").expect("seed");
    git(&root, &["add", "README.md"]);
    git(&root, &["commit", "-m", "seed main"]);

    let remote = create_bare_origin(base);
    let remote_arg = remote.to_string_lossy().to_string();
    git(&root, &["remote", "add", "origin", remote_arg.as_str()]);
    git(&root, &["push", "-u", "origin", "main"]);

    let init = run_cli(&root, ["init"]);
    assert_eq!(init.code, 0, "stderr: {}", init.stderr);
    let create = run_cli(&root, ["create", "First remote sync task"]);
    assert_eq!(create.code, 0, "stderr: {}", create.stderr);

    let sync_result = run_cli(&root, ["sync", "--json"]);
    assert_eq!(sync_result.code, 0, "stderr: {}", sync_result.stderr);
    let envelope: Value = serde_json::from_str(sync_result.stdout.trim()).expect("json envelope");
    assert_eq!(
        envelope
            .get("data")
            .and_then(|data| data.get("pushed"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        envelope
            .get("data")
            .and_then(|data| data.get("has_upstream"))
            .and_then(Value::as_bool),
        Some(true)
    );

    let wt = root.join(".git").join("tsq-sync");
    assert_eq!(
        git_out(
            &wt,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}"
            ]
        ),
        "origin/tsq-sync"
    );
    assert!(!git_out(&remote, &["show-ref", "--heads", "tsq-sync"]).is_empty());
}

#[test]
fn migrate_defaults_to_tsq_sync_worktree_name() {
    let repo = make_repo();
    let root = repo.path();

    let init = run_cli(root, ["init"]);
    assert_eq!(init.code, 0, "stderr: {}", init.stderr);
    let create = run_cli(root, ["create", "Legacy migration task"]);
    assert_eq!(create.code, 0, "stderr: {}", create.stderr);

    init_git_repo_with_identity(root, None);

    let migrate = run_cli(root, ["migrate", "--json"]);
    assert_eq!(migrate.code, 0, "stderr: {}", migrate.stderr);
    let envelope: Value = serde_json::from_str(migrate.stdout.trim()).expect("json envelope");
    assert_eq!(envelope.get("ok").and_then(Value::as_bool), Some(true));
    assert_eq!(
        envelope
            .get("data")
            .and_then(|data| data.get("branch"))
            .and_then(Value::as_str),
        Some("tsq-sync")
    );

    let wt = root.join(".git").join("tsq-sync");
    assert!(wt.join(".tasque").join("events.jsonl").exists());
    let root_events =
        fs::read_to_string(root.join(".tasque").join("events.jsonl")).expect("events");
    assert!(root_events.is_empty(), "expected root events cleared");
}

#[test]
fn migrate_pushes_sync_branch_to_main_upstream_before_clearing_root_events() {
    let repo = make_repo();
    let base = repo.path();
    let root = base.join("repo");
    fs::create_dir(&root).expect("repo dir");

    let init = run_cli(&root, ["init"]);
    assert_eq!(init.code, 0, "stderr: {}", init.stderr);
    let create = run_cli(&root, ["create", "Migrated remote task"]);
    assert_eq!(create.code, 0, "stderr: {}", create.stderr);

    init_git_repo_with_identity(&root, Some("main"));
    git(
        &root,
        &["add", ".tasque/config.json", ".tasque/events.jsonl"],
    );
    git(&root, &["commit", "-m", "seed legacy tasque data"]);

    let remote = create_bare_origin(base);
    let remote_arg = remote.to_string_lossy().to_string();
    git(&root, &["remote", "add", "origin", remote_arg.as_str()]);
    git(&root, &["push", "-u", "origin", "main"]);

    let migrate = run_cli(&root, ["migrate", "--json"]);
    assert_eq!(migrate.code, 0, "stderr: {}", migrate.stderr);

    let wt = root.join(".git").join("tsq-sync");
    assert_eq!(
        git_out(
            &wt,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}"
            ]
        ),
        "origin/tsq-sync"
    );
    assert!(!git_out(&remote, &["show-ref", "--heads", "tsq-sync"]).is_empty());
    let root_events =
        fs::read_to_string(root.join(".tasque").join("events.jsonl")).expect("events");
    assert!(
        root_events.is_empty(),
        "expected root events cleared after remote push"
    );
}

#[test]
fn fresh_clone_fetches_remote_sync_branch_and_creates_worktree() {
    let repo = make_repo();
    let base = repo.path();
    let source = base.join("source");
    fs::create_dir(&source).expect("source dir");

    init_git_repo_with_identity(&source, Some("main"));

    let init = run_cli(&source, ["init"]);
    assert_eq!(init.code, 0, "stderr: {}", init.stderr);
    let create = run_cli(&source, ["create", "Remote sync task"]);
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
    let missing_remote = base.join("missing-origin.git");
    let missing_remote_arg = missing_remote.to_string_lossy().to_string();
    git(
        &clone,
        &["remote", "set-url", "origin", missing_remote_arg.as_str()],
    );

    let list = run_cli(&clone, ["find", "open", "--json"]);
    assert_eq!(list.code, 0, "stderr: {}", list.stderr);
    assert!(
        list.stdout.contains("Remote sync task"),
        "expected cloned repo to read remote sync task:\n{}",
        list.stdout
    );
    assert!(
        clone
            .join(".git")
            .join("tsq-sync")
            .join(".tasque")
            .join("events.jsonl")
            .exists()
    );
    assert_eq!(git_out(&clone, &["branch", "--show-current"]), "main");
    assert!(
        git_out(
            &clone.join(".git").join("tsq-sync"),
            &["branch", "--show-current"]
        )
        .contains("tsq-sync")
    );
}
