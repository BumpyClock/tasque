mod common;

use common::{assert_validation_error, create_task, create_task_with_args, init_repo, run_json};

#[test]
fn create_accepts_variadic_children_under_parent() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let parent = create_task(repo.path(), "Parent");

    let result = run_json(repo.path(), ["create", "--parent", &parent, "A", "B"]);

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"].as_array().expect("tasks");
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["parent_id"].as_str(), Some(parent.as_str()));
}

#[test]
fn create_from_file_accepts_markdown_bullets() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- [ ] Add parser tests\n- Wire CLI command\n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"].as_array().expect("tasks");
    assert_eq!(tasks[0]["title"].as_str(), Some("Add parser tests"));
    assert_eq!(tasks[1]["title"].as_str(), Some("Wire CLI command"));
}

#[test]
fn create_from_file_allocates_root_ids_sequentially_after_high_existing_id() {
    let repo = common::make_repo();
    init_repo(repo.path());
    create_task_with_args(repo.path(), "Existing high root", &["--id", "tsq-42"]);
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- First root\n- Second root\n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"].as_array().expect("tasks");
    assert_eq!(tasks[0]["id"].as_str(), Some("tsq-43"));
    assert_eq!(tasks[1]["id"].as_str(), Some("tsq-44"));
}

#[test]
fn create_from_file_maps_nested_bullets_to_parent_ids() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(
        &file,
        "- Parent A\n  - Child A1\n    - Grandchild A1a\n  - [ ] Child A2\n- Parent B\n",
    )
    .unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"].as_array().expect("tasks");
    assert_eq!(tasks.len(), 5);
    assert_eq!(tasks[0]["title"].as_str(), Some("Parent A"));
    assert_eq!(tasks[1]["title"].as_str(), Some("Child A1"));
    assert_eq!(tasks[2]["title"].as_str(), Some("Grandchild A1a"));
    assert_eq!(tasks[3]["title"].as_str(), Some("Child A2"));
    assert_eq!(tasks[4]["title"].as_str(), Some("Parent B"));

    let parent_a = tasks[0]["id"].as_str().expect("parent id");
    let child_a1 = tasks[1]["id"].as_str().expect("child id");
    assert!(tasks[0].get("parent_id").is_none());
    assert_eq!(tasks[1]["parent_id"].as_str(), Some(parent_a));
    assert_eq!(tasks[2]["parent_id"].as_str(), Some(child_a1));
    assert_eq!(tasks[3]["parent_id"].as_str(), Some(parent_a));
    assert!(tasks[4].get("parent_id").is_none());
}

#[test]
fn create_from_file_nested_bullets_respect_external_parent() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let external_parent = create_task(repo.path(), "Epic");
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- Parent A\n  - Child A1\n- Parent B\n").unwrap();

    let result = run_json(
        repo.path(),
        [
            "create",
            "--parent",
            &external_parent,
            "--from-file",
            "tasks.md",
        ],
    );

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"].as_array().expect("tasks");
    assert_eq!(tasks.len(), 3);
    let parent_a = tasks[0]["id"].as_str().expect("parent A id");
    assert_eq!(
        tasks[0]["parent_id"].as_str(),
        Some(external_parent.as_str())
    );
    assert_eq!(tasks[1]["parent_id"].as_str(), Some(parent_a));
    assert_eq!(
        tasks[2]["parent_id"].as_str(),
        Some(external_parent.as_str())
    );
}

#[test]
fn create_from_file_rejects_tab_indentation() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- Parent\n\t- Child\n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_validation_error(&result);
    assert_eq!(
        result.envelope["error"]["message"].as_str(),
        Some("line 2 tab indentation is not supported")
    );
}

#[test]
fn create_from_file_rejects_odd_indentation() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- Parent\n - Child\n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_validation_error(&result);
    assert_eq!(
        result.envelope["error"]["message"].as_str(),
        Some("line 2 indentation must use multiples of 2 spaces")
    );
}

#[test]
fn create_from_file_rejects_skipped_depth() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- Parent\n    - Grandchild\n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_validation_error(&result);
    assert_eq!(
        result.envelope["error"]["message"].as_str(),
        Some("line 2 indentation jumps from depth 0 to depth 2")
    );
}

#[test]
fn create_from_file_rejects_indented_first_bullet() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "  - Child without parent\n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_validation_error(&result);
    assert_eq!(
        result.envelope["error"]["message"].as_str(),
        Some("line 1 first bullet must not be indented")
    );
}

#[test]
fn create_from_file_rejects_non_bullet_content_with_line_number() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- Parent\nparagraph text\n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_validation_error(&result);
    assert_eq!(
        result.envelope["error"]["message"].as_str(),
        Some("line 2 must be a markdown bullet starting with '- '")
    );
}

#[test]
fn create_from_file_rejects_empty_checkbox_title() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- [ ]   \n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_validation_error(&result);
    assert_eq!(
        result.envelope["error"]["message"].as_str(),
        Some("line 1 task title must not be empty")
    );
}

#[test]
fn edit_updates_metadata_without_status_change() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Old title");

    let result = run_json(
        repo.path(),
        ["edit", &id, "--title", "New title", "--priority", "1"],
    );

    assert_eq!(result.cli.code, 0);
    assert_eq!(
        result.envelope["data"]["task"]["title"].as_str(),
        Some("New title")
    );
    assert_eq!(
        result.envelope["data"]["task"]["priority"].as_u64(),
        Some(1)
    );
}

#[test]
fn assign_sets_assignee_without_status_change() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Open task");

    let result = run_json(repo.path(), ["assign", &id, "--assignee", "alice"]);

    assert_eq!(result.cli.code, 0);
    let task = &result.envelope["data"]["task"];
    assert_eq!(task["assignee"].as_str(), Some("alice"));
    assert_eq!(task["status"].as_str(), Some("open"));
}

#[test]
fn lifecycle_done_accepts_note_and_multiple_ids() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let a = create_task(repo.path(), "A");
    let b = create_task(repo.path(), "B");

    let result = run_json(repo.path(), ["done", &a, &b, "--note", "verified"]);

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"].as_array().expect("tasks");
    assert_eq!(tasks.len(), 2);
    assert!(
        tasks
            .iter()
            .all(|task| task["status"].as_str() == Some("closed"))
    );
    let notes = result.envelope["data"]["notes"].as_array().expect("notes");
    assert_eq!(notes.len(), 2);
    assert!(
        notes
            .iter()
            .all(|note| note["note"]["event_id"].is_string())
    );
}

#[test]
fn lifecycle_note_is_not_written_when_any_target_status_is_invalid() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let open = create_task(repo.path(), "Open");
    let closed = create_task(repo.path(), "Closed");
    let close = run_json(repo.path(), ["done", &closed]);
    assert_eq!(close.cli.code, 0);
    let before_events = std::fs::read_to_string(repo.path().join(".tasque/events.jsonl")).unwrap();

    let result = run_json(
        repo.path(),
        ["done", &open, &closed, "--note", "should not persist"],
    );

    assert_eq!(result.cli.code, 1);
    assert_validation_error(&result);
    let after_events = std::fs::read_to_string(repo.path().join(".tasque/events.jsonl")).unwrap();
    assert_eq!(after_events, before_events);
    let show_open = run_json(repo.path(), ["show", &open]);
    assert_eq!(
        show_open.envelope["data"]["task"]["status"].as_str(),
        Some("open")
    );
    assert_eq!(
        show_open.envelope["data"]["task"]["notes"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
}

#[test]
fn defer_note_json_includes_note_event_id() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Park later");

    let result = run_json(repo.path(), ["defer", &id, "--note", "waiting"]);

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"].as_array().expect("tasks");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["status"].as_str(), Some("deferred"));
    let notes = result.envelope["data"]["notes"].as_array().expect("notes");
    assert_eq!(notes.len(), 1);
    assert!(notes[0]["note"]["event_id"].is_string());
}

#[test]
fn lifecycle_multi_id_verbs_reject_empty_ids() {
    let repo = common::make_repo();
    init_repo(repo.path());

    for command in ["done", "reopen", "cancel"] {
        let result = run_json(repo.path(), [command]);
        assert_eq!(result.cli.code, 1, "{command} should reject empty ids");
        assert_validation_error(&result);
    }
}

#[test]
fn find_ready_filters_assignee_after_ready_lane_semantics() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let ready_coding = create_task_with_args(repo.path(), "Ready coding", &["--planned"]);
    let blocked_coding = create_task_with_args(repo.path(), "Blocked coding", &["--planned"]);
    let blocker = create_task(repo.path(), "Open blocker");
    let planning = create_task_with_args(repo.path(), "Planning lane", &["--needs-plan"]);

    for id in [&ready_coding, &blocked_coding, &planning] {
        let assign = run_json(repo.path(), ["assign", id, "--assignee", "alice"]);
        assert_eq!(assign.cli.code, 0);
    }
    let block = run_json(repo.path(), ["block", &blocked_coding, "by", &blocker]);
    assert_eq!(block.cli.code, 0);

    let coding = run_json(
        repo.path(),
        ["find", "ready", "--lane", "coding", "--assignee", "alice"],
    );
    assert_eq!(coding.cli.code, 0);
    let coding_ids = common::ids_from_task_list(&coding.envelope);
    assert_eq!(coding_ids, vec![ready_coding.clone()]);

    let planning_result = run_json(
        repo.path(),
        ["find", "ready", "--lane", "planning", "--assignee", "alice"],
    );
    assert_eq!(planning_result.cli.code, 0);
    let planning_ids = common::ids_from_task_list(&planning_result.envelope);
    assert_eq!(planning_ids, vec![planning]);
}

#[test]
fn find_search_replaces_search() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Needle task");
    create_task(repo.path(), "Other task");

    let result = run_json(repo.path(), ["find", "search", "Needle"]);

    assert_eq!(result.cli.code, 0);
    let ids: Vec<&str> = result.envelope["data"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|task| task["id"].as_str())
        .collect();
    assert_eq!(ids, vec![id.as_str()]);
}

#[test]
fn find_search_full_prints_full_task_details_for_human_output() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Needle task");

    let result = common::run_cli(repo.path(), ["find", "search", "Needle", "--full"]);

    assert_eq!(result.code, 0);
    assert!(result.stdout.contains(&id), "stdout:\n{}", result.stdout);
    assert!(
        result.stdout.contains("notes=0"),
        "stdout:\n{}",
        result.stdout
    );
}
