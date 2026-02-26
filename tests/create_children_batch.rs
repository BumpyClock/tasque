mod common;

use common::{init_repo, run_json};
use serde_json::Value;

#[test]
fn create_supports_multiple_children_for_single_parent() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let parent = common::create_task(repo.path(), "Parent task");
    let result = run_json(
        repo.path(),
        [
            "create",
            "--parent",
            parent.as_str(),
            "--child",
            "Draft CLI UX",
            "--child",
            "Add service tests",
        ],
    );

    assert_eq!(result.cli.code, 0);
    let tasks = result
        .envelope
        .get("data")
        .and_then(|value| value.get("tasks"))
        .and_then(Value::as_array)
        .expect("expected data.tasks array");
    assert_eq!(tasks.len(), 2);

    let ids: Vec<String> = tasks
        .iter()
        .filter_map(|task| task.get("id").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect();
    assert_eq!(ids, vec![format!("{}.1", parent), format!("{}.2", parent)]);

    let parent_ids: Vec<String> = tasks
        .iter()
        .filter_map(|task| task.get("parent_id").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect();
    assert_eq!(parent_ids, vec![parent.clone(), parent]);
}

#[test]
fn create_rejects_child_without_parent() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json(repo.path(), ["create", "--child", "Needs parent"]);
    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|value| value.get("code"))
            .and_then(Value::as_str),
        Some("VALIDATION_ERROR")
    );
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|value| value.get("message"))
            .and_then(Value::as_str),
        Some("--child requires --parent")
    );
}

#[test]
fn create_single_task_output_shape_stays_stable() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json(repo.path(), ["create", "Single task"]);
    assert_eq!(result.cli.code, 0);

    let data = result
        .envelope
        .get("data")
        .expect("expected data field in create response");
    assert!(
        data.get("task").is_some(),
        "expected data.task for single create"
    );
    assert!(
        data.get("tasks").is_none(),
        "single create must not return data.tasks"
    );
}

#[test]
fn create_ensure_is_idempotent_for_root_task() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let first = run_json(repo.path(), ["create", "Root task", "--ensure"]);
    assert_eq!(first.cli.code, 0);
    let first_id = first
        .envelope
        .get("data")
        .and_then(|value| value.get("task"))
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        .expect("expected data.task.id on first ensure create")
        .to_string();

    let second = run_json(repo.path(), ["create", "Root task", "--ensure"]);
    assert_eq!(second.cli.code, 0);
    let second_id = second
        .envelope
        .get("data")
        .and_then(|value| value.get("task"))
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        .expect("expected data.task.id on second ensure create")
        .to_string();
    assert_eq!(first_id, second_id);

    let listed = run_json(repo.path(), ["list"]);
    assert_eq!(listed.cli.code, 0);
    let root_matches = listed
        .envelope
        .get("data")
        .and_then(|value| value.get("tasks"))
        .and_then(Value::as_array)
        .expect("expected data.tasks array")
        .iter()
        .filter(|task| task.get("title").and_then(Value::as_str) == Some("Root task"))
        .count();
    assert_eq!(root_matches, 1);
}

#[test]
fn create_ensure_is_idempotent_for_parent_children_batch() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let parent = common::create_task(repo.path(), "Parent task");
    let cmd = [
        "create",
        "--parent",
        parent.as_str(),
        "--child",
        "Design API contract",
        "--child",
        "Implement logic",
        "--ensure",
    ];

    let first = run_json(repo.path(), cmd);
    assert_eq!(first.cli.code, 0);
    let second = run_json(repo.path(), cmd);
    assert_eq!(second.cli.code, 0);

    let expected_ids = vec![format!("{}.1", parent), format!("{}.2", parent)];
    let first_ids: Vec<String> = first
        .envelope
        .get("data")
        .and_then(|value| value.get("tasks"))
        .and_then(Value::as_array)
        .expect("expected first data.tasks")
        .iter()
        .filter_map(|task| task.get("id").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect();
    let second_ids: Vec<String> = second
        .envelope
        .get("data")
        .and_then(|value| value.get("tasks"))
        .and_then(Value::as_array)
        .expect("expected second data.tasks")
        .iter()
        .filter_map(|task| task.get("id").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect();
    assert_eq!(first_ids, expected_ids);
    assert_eq!(second_ids, expected_ids);

    let listed = run_json(repo.path(), ["list"]);
    assert_eq!(listed.cli.code, 0);
    let child_count = listed
        .envelope
        .get("data")
        .and_then(|value| value.get("tasks"))
        .and_then(Value::as_array)
        .expect("expected data.tasks array")
        .iter()
        .filter(|task| task.get("parent_id").and_then(Value::as_str) == Some(parent.as_str()))
        .count();
    assert_eq!(child_count, 2);
}

#[test]
fn create_rejects_ensure_with_explicit_id() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json(
        repo.path(),
        [
            "create",
            "With explicit id",
            "--id",
            "tsq-aaaaaaaa",
            "--ensure",
        ],
    );
    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|value| value.get("code"))
            .and_then(Value::as_str),
        Some("VALIDATION_ERROR")
    );
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|value| value.get("message"))
            .and_then(Value::as_str),
        Some("cannot combine --ensure with --id")
    );
}
