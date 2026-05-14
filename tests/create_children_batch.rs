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
            "Draft CLI UX",
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
fn create_supports_multiple_root_tasks_without_parent() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json(repo.path(), ["create", "Root A", "Root B"]);
    assert_eq!(result.cli.code, 0);
    let tasks = result
        .envelope
        .get("data")
        .and_then(|value| value.get("tasks"))
        .and_then(Value::as_array)
        .expect("expected data.tasks array");
    assert_eq!(tasks.len(), 2);
    assert!(tasks.iter().all(|task| task.get("parent_id").is_none()));
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

    let listed = run_json(repo.path(), ["find", "open"]);
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
        "Design API contract",
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

    let listed = run_json(repo.path(), ["find", "open"]);
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
fn create_ensure_is_idempotent_for_nested_file_batch() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let file = repo.path().join("tasks.md");
    std::fs::write(
        &file,
        "- Parent A\n  - Child A1\n- Parent B\n  - Child B1\n",
    )
    .unwrap();
    let cmd = ["create", "--from-file", "tasks.md", "--ensure"];

    let first = run_json(repo.path(), cmd);
    assert_eq!(first.cli.code, 0);
    let second = run_json(repo.path(), cmd);
    assert_eq!(second.cli.code, 0);

    let first_tasks = first.envelope["data"]["tasks"]
        .as_array()
        .expect("first tasks");
    let second_tasks = second.envelope["data"]["tasks"]
        .as_array()
        .expect("second tasks");
    let first_ids: Vec<&str> = first_tasks
        .iter()
        .map(|task| task["id"].as_str().expect("first id"))
        .collect();
    let second_ids: Vec<&str> = second_tasks
        .iter()
        .map(|task| task["id"].as_str().expect("second id"))
        .collect();
    assert_eq!(first_ids, second_ids);

    let listed = run_json(repo.path(), ["find", "open"]);
    assert_eq!(listed.cli.code, 0);
    let all_tasks = listed.envelope["data"]["tasks"].as_array().expect("tasks");
    let matching_count = all_tasks
        .iter()
        .filter(|task| {
            matches!(
                task["title"].as_str(),
                Some("Parent A") | Some("Child A1") | Some("Parent B") | Some("Child B1")
            )
        })
        .count();
    assert_eq!(matching_count, 4);
}

#[test]
fn create_ensure_scopes_nested_titles_to_resolved_parent() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let file = repo.path().join("tasks.md");
    std::fs::write(
        &file,
        "- Parent A\n  - Shared child\n- Parent B\n  - Shared child\n",
    )
    .unwrap();
    let result = run_json(
        repo.path(),
        ["create", "--from-file", "tasks.md", "--ensure"],
    );

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"].as_array().expect("tasks");
    assert_eq!(tasks.len(), 4);
    let parent_a = tasks[0]["id"].as_str().expect("parent A");
    let child_a = tasks[1]["id"].as_str().expect("child A");
    let parent_b = tasks[2]["id"].as_str().expect("parent B");
    let child_b = tasks[3]["id"].as_str().expect("child B");

    assert_eq!(tasks[1]["parent_id"].as_str(), Some(parent_a));
    assert_eq!(tasks[3]["parent_id"].as_str(), Some(parent_b));
    assert_ne!(child_a, child_b);
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

#[test]
fn ensure_deduplicates_identical_incoming_root_tasks() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json(
        repo.path(),
        ["create", "--ensure", "Root task", "Root task"],
    );

    assert_eq!(result.cli.code, 0);
    // Verify only one open task with that title exists.
    let listed = run_json(repo.path(), ["find", "open"]);
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
    assert_eq!(
        root_matches, 1,
        "ensure must deduplicate identical incoming root tasks"
    );
}

#[test]
fn ensure_from_file_deduplicates_identical_children_under_same_parent() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- Parent\n  - Child task\n  - Child task\n").unwrap();

    let result = run_json(
        repo.path(),
        ["create", "--from-file", "tasks.md", "--ensure"],
    );

    assert_eq!(result.cli.code, 0);
    let listed = run_json(repo.path(), ["find", "open"]);
    assert_eq!(listed.cli.code, 0);
    let child_matches = listed
        .envelope
        .get("data")
        .and_then(|value| value.get("tasks"))
        .and_then(Value::as_array)
        .expect("expected data.tasks array")
        .iter()
        .filter(|task| task.get("title").and_then(Value::as_str) == Some("Child task"))
        .count();
    assert_eq!(
        child_matches, 1,
        "ensure must deduplicate identical children under same parent"
    );
}
