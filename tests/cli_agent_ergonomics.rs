mod common;

use common::{create_task, init_repo, run_cli, run_json};

#[test]
fn commands_do_not_discover_parent_tasque_store() {
    let repo = common::make_repo();
    init_repo(repo.path());
    std::fs::create_dir(repo.path().join("child")).unwrap();

    let result = run_json(&repo.path().join("child"), ["find", "open"]);

    assert_eq!(result.cli.code, 2);
    assert_eq!(result.envelope["error"]["code"].as_str(), Some("NO_STORE"));
}

#[test]
fn root_flag_targets_store_without_ancestor_discovery() {
    let repo = common::make_repo();
    init_repo(repo.path());
    std::fs::create_dir(repo.path().join("child")).unwrap();

    let result = run_cli(
        &repo.path().join("child"),
        ["--root", repo.path().to_str().unwrap(), "root", "--plain"],
    );

    assert_eq!(result.code, 0);
    assert_eq!(
        std::fs::canonicalize(result.stdout.trim()).unwrap(),
        std::fs::canonicalize(repo.path()).unwrap()
    );
}

#[test]
fn workon_claims_and_starts_task_in_one_command() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Workon target");

    let result = run_json(repo.path(), ["workon", &id, "--assignee", "agent"]);

    assert_eq!(result.cli.code, 0);
    let task = &result.envelope["data"]["task"];
    assert_eq!(task["status"].as_str(), Some("in_progress"));
    assert_eq!(task["assignee"].as_str(), Some("agent"));
}

#[test]
fn show_defaults_to_context_and_selector_flags_narrow_output() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let child = create_task(repo.path(), "Child");
    let blocker = create_task(repo.path(), "Blocker");
    let dep = run_json(repo.path(), ["block", &child, "by", &blocker]);
    assert_eq!(dep.cli.code, 0);

    let default_show = run_cli(repo.path(), ["show", &child]);
    assert_eq!(default_show.code, 0);
    assert!(default_show.stdout.contains("blockers="));

    let deps_only = run_cli(repo.path(), ["show", &child, "--deps", "--plain"]);
    assert_eq!(deps_only.code, 0);
    assert!(deps_only.stdout.contains("\nblocker\t"));
    assert!(!deps_only.stdout.contains("spec_path\t"));
}

#[test]
fn plan_apply_creates_labeled_child_tasks_under_parent() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let parent = create_task(repo.path(), "Parent");
    std::fs::write(
        repo.path().join("plan.md"),
        "- Build root handling #cli\n  - [ ] Add root tests #test\n",
    )
    .unwrap();

    let result = run_json(
        repo.path(),
        ["plan", &parent, "--from", "plan.md", "--apply"],
    );

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["plan"]["tasks"]
        .as_array()
        .expect("tasks");
    assert_eq!(tasks.len(), 2);
    let root = tasks[0]["id"].as_str().expect("root id");
    assert_eq!(tasks[0]["parent_id"].as_str(), Some(parent.as_str()));
    assert_eq!(tasks[0]["labels"][0].as_str(), Some("cli"));
    assert_eq!(tasks[1]["parent_id"].as_str(), Some(root));
    assert_eq!(tasks[1]["labels"][0].as_str(), Some("test"));
    assert_eq!(tasks[0]["planning_state"].as_str(), Some("needs_planning"));
}
