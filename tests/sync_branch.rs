mod common;

use common::{git, git_output, init_git_repo_with_identity, make_repo, run_cli};
use serde_json::Value;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn git_out(repo: &std::path::Path, args: &[&str]) -> String {
    let output = git_output(repo, args);
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Assert that events were migrated out of the main tree into the sync
/// worktree: root `events.jsonl` cleared, sync worktree `events.jsonl`
/// populated. Shared by the implicit- and explicit-migration tests.
fn assert_events_migrated_to_sync_worktree(root: &std::path::Path) {
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

const EXPECTED_MERGE_DRIVER_SUFFIX: &str = " merge-driver %O %A %B";

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
    assert!(
        driver.ends_with(EXPECTED_MERGE_DRIVER_SUFFIX),
        "expected merge driver to be configured after worktree materializes, got: {driver}"
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
    assert!(
        restored.ends_with(EXPECTED_MERGE_DRIVER_SUFFIX),
        "expected `tsq sync` to re-ensure the merge driver config, got: {restored}"
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

    let config_text = fs::read_to_string(root.join(".tasque").join("config.json")).expect("config");
    let config: Value = serde_json::from_str(&config_text).expect("config must be valid JSON");
    assert!(
        config.get("sync_branch").is_none(),
        "expected legacy config without sync_branch:\n{}",
        config_text
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

    assert_events_migrated_to_sync_worktree(&root);
}

/// Explicit `tsq migrate` keeps push failures fatal and preserves the original
/// main-tree events when the required push fails. The sync worktree commit has
/// landed locally, so a later successful migrate can dedupe and clear safely.
#[test]
fn explicit_migrate_fails_on_unreachable_origin_after_local_migration() {
    let repo = make_repo();
    let base = repo.path();
    let root = make_legacy_repo_with_unreachable_origin(base, "Offline explicit task");

    let migrate = run_cli(&root, ["migrate", "--json"]);
    assert_eq!(
        migrate.code, 2,
        "expected explicit migrate to fail with storage/IO error when push fails\nstdout:\n{}\nstderr:\n{}",
        migrate.stdout, migrate.stderr
    );
    let envelope: Value = serde_json::from_str(migrate.stdout.trim()).expect("json envelope");
    assert_eq!(envelope.get("ok").and_then(Value::as_bool), Some(false));

    let root_events =
        fs::read_to_string(root.join(".tasque").join("events.jsonl")).expect("root events");
    assert!(
        !root_events.is_empty(),
        "expected main-tree events preserved after required push failure"
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

/// Read the sync-worktree `events.jsonl` for a clone/repo whose sync branch
/// lives at `<root>/.git/tsq-sync`.
fn read_sync_worktree_events(root: &std::path::Path) -> String {
    fs::read_to_string(
        root.join(".git")
            .join("tsq-sync")
            .join(".tasque")
            .join("events.jsonl"),
    )
    .unwrap_or_default()
}

/// Seed a `source` repo and push only `main` to a fresh bare `origin`.
/// Returns `(source_path, origin_path)`; `tsq-sync` exists locally but not on
/// the remote until a sync publishes it.
fn seed_source_with_origin_main_only(
    base: &std::path::Path,
    title: &str,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let source = base.join("source");
    fs::create_dir(&source).expect("source dir");
    init_git_repo_with_identity(&source, Some("main"));

    let init = run_cli(&source, ["init"]);
    assert_eq!(init.code, 0, "stderr: {}", init.stderr);
    let create = run_cli(&source, ["create", title, "--force"]);
    assert_eq!(create.code, 0, "stderr: {}", create.stderr);

    git(&source, &["add", ".tasque/config.json", ".gitattributes"]);
    git(&source, &["commit", "-m", "seed main config"]);

    let remote = create_bare_origin(base);
    let remote_arg = remote.to_string_lossy().to_string();
    git(&source, &["remote", "add", "origin", remote_arg.as_str()]);
    git(&source, &["push", "origin", "HEAD:main"]);
    (source, remote)
}

/// Seed a `source` repo (main + tsq-sync) and push both branches to a fresh
/// bare `origin`. Returns `(source_path, origin_path)`.
fn seed_source_with_origin(
    base: &std::path::Path,
    title: &str,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let (source, remote) = seed_source_with_origin_main_only(base, title);
    git(&source, &["push", "origin", "tsq-sync"]);
    (source, remote)
}

/// Clone `remote` into `<base>/<name>` and configure a test git identity.
fn clone_from_origin(
    base: &std::path::Path,
    remote: &std::path::Path,
    name: &str,
) -> std::path::PathBuf {
    let clone = base.join(name);
    let clone_arg = clone.to_string_lossy().to_string();
    let remote_arg = remote.to_string_lossy().to_string();
    git(base, &["clone", remote_arg.as_str(), clone_arg.as_str()]);
    git(&clone, &["config", "user.name", "rust-test"]);
    git(&clone, &["config", "user.email", "rust-test@example.com"]);
    clone
}

#[cfg(unix)]
fn write_executable_hook(path: &std::path::Path, script: String) {
    fs::write(path, script).expect("write git hook");
    let mut permissions = fs::metadata(path).expect("hook metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod git hook");
}

/// Two clones create distinct tasks and sync. The clone that pushes second must
/// fetch + merge the first clone's events before pushing (local-first two-way
/// sync), and a subsequent sync on the first clone must converge to both events.
/// Explicit ids avoid the sequential-id collision that would otherwise force a
/// merge conflict between independently created tasks.
#[test]
fn two_clone_sync_converges_after_non_fast_forward() {
    let repo = make_repo();
    let base = repo.path();
    let (_source, remote) = seed_source_with_origin(base, "Seed task");

    let clone_a = clone_from_origin(base, &remote, "cloneA");
    let clone_b = clone_from_origin(base, &remote, "cloneB");

    let ca = run_cli(
        &clone_a,
        ["create", "Task A", "--id", "tsq-aaaa1111", "--force"],
    );
    assert_eq!(ca.code, 0, "stderr: {}", ca.stderr);
    let sa = run_cli(&clone_a, ["sync"]);
    assert_eq!(sa.code, 0, "clone A first sync\nstderr: {}", sa.stderr);

    let cb = run_cli(
        &clone_b,
        ["create", "Task B", "--id", "tsq-bbbb2222", "--force"],
    );
    assert_eq!(cb.code, 0, "stderr: {}", cb.stderr);
    // Clone B is behind origin (A pushed first): sync must fetch + merge + push.
    let sb = run_cli(&clone_b, ["sync"]);
    assert_eq!(
        sb.code, 0,
        "clone B sync should fetch+merge+push after non-ff\nstdout:\n{}\nstderr:\n{}",
        sb.stdout, sb.stderr
    );

    // Clone A pulls in clone B's task on its next sync.
    let sa2 = run_cli(&clone_a, ["sync"]);
    assert_eq!(sa2.code, 0, "clone A second sync\nstderr: {}", sa2.stderr);

    let events_a = read_sync_worktree_events(&clone_a);
    let events_b = read_sync_worktree_events(&clone_b);
    for id in ["tsq-aaaa1111", "tsq-bbbb2222"] {
        assert!(events_a.contains(id), "clone A missing {id}:\n{events_a}");
        assert!(events_b.contains(id), "clone B missing {id}:\n{events_b}");
    }
}

/// A third clone can advance the remote between our fetch/merge and push. Sync
/// must classify that non-fast-forward rejection as recoverable, then fetch,
/// merge, and push again.
#[cfg(unix)]
#[test]
fn sync_retries_when_remote_advances_during_push() {
    let repo = make_repo();
    let base = repo.path();
    let (_source, remote) = seed_source_with_origin(base, "Seed task");

    let target = clone_from_origin(base, &remote, "target");
    let advancer = clone_from_origin(base, &remote, "advancer");

    let target_create = run_cli(
        &target,
        ["create", "Target task", "--id", "tsq-cccc1111", "--force"],
    );
    assert_eq!(target_create.code, 0, "stderr: {}", target_create.stderr);
    let advancer_create = run_cli(
        &advancer,
        ["create", "Advancer task", "--id", "tsq-cccc2222", "--force"],
    );
    assert_eq!(
        advancer_create.code, 0,
        "stderr: {}",
        advancer_create.stderr
    );

    let flag = base.join("pre-push-fired");
    let hook = target.join(".git").join("hooks").join("pre-push");
    let script = format!(
        "#!/bin/sh\nif [ ! -f '{flag}' ]; then\n  touch '{flag}'\n  unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE\n  git -C '{advancer_wt}' push origin tsq-sync >/dev/null 2>&1 || exit 1\nfi\nexit 0\n",
        flag = flag.display(),
        advancer_wt = advancer.join(".git").join("tsq-sync").display()
    );
    write_executable_hook(&hook, script);

    let sync = run_cli(&target, ["sync"]);
    assert_eq!(
        sync.code, 0,
        "sync should retry after pre-push remote advance\nstdout:\n{}\nstderr:\n{}",
        sync.stdout, sync.stderr
    );
    assert!(flag.exists(), "pre-push hook should have advanced remote");

    let target_events = read_sync_worktree_events(&target);
    for id in ["tsq-cccc1111", "tsq-cccc2222"] {
        assert!(
            target_events.contains(id),
            "target should contain retried merge id {id}:\n{target_events}"
        );
    }
}

/// A second clone can create the remote sync branch after our `ls-remote`
/// check but before the first publish push. That first rejection should reuse
/// the normal fetch/merge/retry flow rather than fail the whole sync.
#[cfg(unix)]
#[test]
fn sync_retries_when_remote_branch_appears_during_publish() {
    let repo = make_repo();
    let base = repo.path();
    let (target, remote) = seed_source_with_origin_main_only(base, "Seed task");
    let remote_arg = remote.to_string_lossy().to_string();
    let advancer = clone_from_origin(base, &target, "advancer");
    git(
        &advancer,
        &["remote", "set-url", "origin", remote_arg.as_str()],
    );

    let target_create = run_cli(
        &target,
        ["create", "Target task", "--id", "tsq-cccc5555", "--force"],
    );
    assert_eq!(target_create.code, 0, "stderr: {}", target_create.stderr);
    let advancer_create = run_cli(
        &advancer,
        ["create", "Advancer task", "--id", "tsq-cccc6666", "--force"],
    );
    assert_eq!(
        advancer_create.code, 0,
        "stderr: {}",
        advancer_create.stderr
    );

    let flag = base.join("publish-pre-push-fired");
    let hook = target.join(".git").join("hooks").join("pre-push");
    let script = format!(
        "#!/bin/sh\nif [ ! -f '{flag}' ]; then\n  touch '{flag}'\n  unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE\n  git -C '{advancer_wt}' push origin tsq-sync >/dev/null 2>&1 || exit 1\nfi\nexit 0\n",
        flag = flag.display(),
        advancer_wt = advancer.join(".git").join("tsq-sync").display()
    );
    write_executable_hook(&hook, script);

    let sync = run_cli(&target, ["sync", "--json"]);
    assert_eq!(
        sync.code, 0,
        "publish-path rejection should fall through to retry\nstdout:\n{}\nstderr:\n{}",
        sync.stdout, sync.stderr
    );
    assert!(
        flag.exists(),
        "pre-push hook should have created remote branch"
    );
    let envelope: Value = serde_json::from_str(sync.stdout.trim()).expect("json envelope");
    let data = envelope.get("data").expect("data");
    assert_eq!(data.get("pushed").and_then(Value::as_bool), Some(true));
    assert_eq!(
        data.get("has_upstream").and_then(Value::as_bool),
        Some(true)
    );

    let target_events = read_sync_worktree_events(&target);
    for id in ["tsq-cccc5555", "tsq-cccc6666"] {
        assert!(
            target_events.contains(id),
            "target should contain publish-race merge id {id}:\n{target_events}"
        );
    }
}

/// If the remote advances before every push attempt, sync must stop at the
/// documented retry cap and surface a structured storage error instead of
/// spinning forever or reporting success.
#[cfg(unix)]
#[test]
fn sync_stops_after_repeated_remote_advances() {
    let repo = make_repo();
    let base = repo.path();
    let (_source, remote) = seed_source_with_origin(base, "Seed task");

    let target = clone_from_origin(base, &remote, "target");
    let advancer = clone_from_origin(base, &remote, "advancer");

    let target_create = run_cli(
        &target,
        ["create", "Target task", "--id", "tsq-cccc3333", "--force"],
    );
    assert_eq!(target_create.code, 0, "stderr: {}", target_create.stderr);
    let advancer_create = run_cli(
        &advancer,
        ["create", "Advancer task", "--id", "tsq-cccc4444", "--force"],
    );
    assert_eq!(
        advancer_create.code, 0,
        "stderr: {}",
        advancer_create.stderr
    );

    let count_file = base.join("pre-push-count");
    let hook = target.join(".git").join("hooks").join("pre-push");
    let script = format!(
        r#"#!/bin/sh
count=$(cat '{count_file}' 2>/dev/null || echo 0)
count=$((count + 1))
printf '%s' "$count" > '{count_file}'
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE

event_id=$(printf '01HOOK%018d' "$count")
task_id=$(printf 'tsq-eeee%04d' "$count")
cat >> '{advancer_wt}/.tasque/events.jsonl' <<EOF
{{"id":"$event_id","ts":"2026-01-01T00:00:00Z","actor":"hook","type":"task.created","task_id":"$task_id","payload":{{"title":"Hook task $count","kind":"task","priority":1,"status":"open","planning_state":"needs_planning"}}}}
EOF

git -C '{advancer_wt}' add .tasque/events.jsonl || exit 1
git -C '{advancer_wt}' commit -m "hook advance $count" >/dev/null 2>&1 || exit 1
git -C '{advancer_wt}' push origin tsq-sync >/dev/null 2>&1 || exit 1
exit 0
"#,
        count_file = count_file.display(),
        advancer_wt = advancer.join(".git").join("tsq-sync").display()
    );
    write_executable_hook(&hook, script);

    let sync = run_cli(&target, ["sync", "--json"]);
    assert_eq!(
        sync.code, 2,
        "sync should stop after repeated remote advances\nstdout:\n{}\nstderr:\n{}",
        sync.stdout, sync.stderr
    );
    let envelope: Value = serde_json::from_str(sync.stdout.trim()).expect("json envelope");
    let error = envelope.get("error").expect("error object");
    assert_eq!(
        error.get("code").and_then(Value::as_str),
        Some("SYNC_PUSH_REJECTED")
    );
    assert_eq!(
        error
            .get("details")
            .and_then(|details| details.get("attempts"))
            .and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        fs::read_to_string(&count_file).expect("pre-push count"),
        "3",
        "pre-push hook should have advanced remote once per attempt"
    );
}

/// With an `origin` remote that has no sync branch yet, `tsq sync` publishes the
/// local sync branch with `push -u` (no upstream configured beforehand).
#[test]
fn sync_publishes_sync_branch_when_origin_has_no_upstream() {
    let repo = make_repo();
    let base = repo.path();
    let (source, _remote) = seed_source_with_origin_main_only(base, "Publish task");

    let pre = git_output(&source, &["ls-remote", "--heads", "origin", "tsq-sync"]);
    assert!(
        String::from_utf8_lossy(&pre.stdout).trim().is_empty(),
        "expected no remote sync branch before sync"
    );

    let sync = run_cli(&source, ["sync", "--json"]);
    assert_eq!(sync.code, 0, "stderr: {}", sync.stderr);
    let envelope: Value = serde_json::from_str(sync.stdout.trim()).expect("json envelope");
    let data = envelope.get("data").expect("data");
    assert_eq!(data.get("pushed").and_then(Value::as_bool), Some(true));
    assert_eq!(
        data.get("has_upstream").and_then(Value::as_bool),
        Some(true)
    );

    let post = git_out(&source, &["ls-remote", "--heads", "origin", "tsq-sync"]);
    assert!(
        post.contains("refs/heads/tsq-sync"),
        "expected sync branch published to origin:\n{post}"
    );
}

/// Two clones create the SAME task id with divergent payloads. The second sync
/// hits the tasque-events merge driver conflict, leaves the merge in progress,
/// and returns a structured `SYNC_MERGE_CONFLICT` error carrying the conflicted
/// paths and worktree path. Rerunning while conflicts remain re-emits the same
/// error; resolving + rerunning finalizes the merge and pushes.
#[test]
fn sync_conflict_returns_structured_error_and_resumes() {
    let repo = make_repo();
    let base = repo.path();
    let (_source, remote) = seed_source_with_origin(base, "Seed task");

    let clone_a = clone_from_origin(base, &remote, "cloneA");
    let clone_b = clone_from_origin(base, &remote, "cloneB");

    let ca = run_cli(
        &clone_a,
        ["create", "Title A", "--id", "tsq-dddd0000", "--force"],
    );
    assert_eq!(ca.code, 0, "stderr: {}", ca.stderr);
    let sa = run_cli(&clone_a, ["sync"]);
    assert_eq!(sa.code, 0, "clone A sync\nstderr: {}", sa.stderr);

    let cb = run_cli(
        &clone_b,
        ["create", "Title B", "--id", "tsq-dddd0000", "--force"],
    );
    assert_eq!(cb.code, 0, "stderr: {}", cb.stderr);

    // Clone B fetches A's divergent event for the same id -> merge conflict.
    let sb = run_cli(&clone_b, ["sync", "--json"]);
    assert_eq!(
        sb.code, 1,
        "expected conflict exit code 1\nstdout:\n{}\nstderr:\n{}",
        sb.stdout, sb.stderr
    );
    let envelope: Value = serde_json::from_str(sb.stdout.trim()).expect("json envelope");
    let error = envelope.get("error").expect("error object");
    assert_eq!(
        error.get("code").and_then(Value::as_str),
        Some("SYNC_MERGE_CONFLICT")
    );
    let details = error.get("details").expect("conflict details");
    let paths = details
        .get("conflicted_paths")
        .and_then(Value::as_array)
        .expect("conflicted_paths array");
    assert!(
        paths
            .iter()
            .filter_map(Value::as_str)
            .any(|p| p.contains("events.jsonl")),
        "expected events.jsonl among conflicted paths: {paths:?}"
    );
    assert!(
        details
            .get("worktree_path")
            .and_then(Value::as_str)
            .is_some(),
        "expected worktree_path in conflict details"
    );

    let worktree = clone_b.join(".git").join("tsq-sync");
    assert!(
        worktree.join(".git").exists() && worktree.join(".tasque").exists(),
        "expected merge left in worktree"
    );

    // `--no-push` must not stage and commit unresolved conflict markers.
    let sb_no_push = run_cli(&clone_b, ["sync", "--no-push", "--json"]);
    assert_eq!(
        sb_no_push.code, 1,
        "expected no-push conflict guard\nstdout:\n{}\nstderr:\n{}",
        sb_no_push.stdout, sb_no_push.stderr
    );
    let no_push_envelope: Value =
        serde_json::from_str(sb_no_push.stdout.trim()).expect("json envelope");
    assert_eq!(
        no_push_envelope
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(Value::as_str),
        Some("SYNC_MERGE_CONFLICT")
    );
    let merge_head_still_present =
        git_output(&worktree, &["rev-parse", "-q", "--verify", "MERGE_HEAD"]);
    assert!(
        merge_head_still_present.status.success(),
        "expected MERGE_HEAD to remain after guarded --no-push"
    );

    // Rerun while conflicts remain -> same structured conflict error.
    let sb_again = run_cli(&clone_b, ["sync", "--json"]);
    assert_eq!(
        sb_again.code, 1,
        "expected repeated conflict\nstderr: {}",
        sb_again.stderr
    );
    let envelope_again: Value =
        serde_json::from_str(sb_again.stdout.trim()).expect("json envelope");
    assert_eq!(
        envelope_again
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(Value::as_str),
        Some("SYNC_MERGE_CONFLICT")
    );

    // Resolve by keeping clone B's version, then rerun: merge finalizes + pushes.
    git(&worktree, &["checkout", "--ours", ".tasque/events.jsonl"]);
    git(&worktree, &["add", ".tasque/events.jsonl"]);
    let sb_resolved = run_cli(&clone_b, ["sync"]);
    assert_eq!(
        sb_resolved.code, 0,
        "expected resolved sync to succeed\nstdout:\n{}\nstderr:\n{}",
        sb_resolved.stdout, sb_resolved.stderr
    );
    // Merge is no longer in progress once finalized.
    let merge_head = git_output(&worktree, &["rev-parse", "-q", "--verify", "MERGE_HEAD"]);
    assert!(
        !merge_head.status.success(),
        "expected MERGE_HEAD cleared after resolved sync"
    );
}
