mod common;

use common::{create_task, init_repo, run_cli, run_json};
use tasque::domain::similarity::DEFAULT_SIMILARITY_MIN_SCORE;

#[test]
fn create_projects_stable_alias_from_title() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let id = create_task(repo.path(), "Improve task search warnings");
    let show = run_json(repo.path(), ["show", &id]);

    assert_eq!(
        show.envelope["data"]["task"]["alias"].as_str(),
        Some("improve-task-search-warnings")
    );
}

#[test]
fn alias_collision_gets_numeric_suffix() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let first = create_task(repo.path(), "Improve task search warnings");
    let second =
        common::create_task_with_args(repo.path(), "Improve task search warnings!", &["--force"]);

    let first_show = run_json(repo.path(), ["show", &first]);
    let second_show = run_json(repo.path(), ["show", &second]);

    assert_eq!(
        first_show.envelope["data"]["task"]["alias"].as_str(),
        Some("improve-task-search-warnings")
    );
    assert_eq!(
        second_show.envelope["data"]["task"]["alias"].as_str(),
        Some("improve-task-search-warnings-2")
    );
}

#[test]
fn title_edit_does_not_recompute_alias() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let id = create_task(repo.path(), "Original alias title");
    let edit = run_json(repo.path(), ["edit", &id, "--title", "Renamed task"]);
    assert_eq!(edit.cli.code, 0);
    let show = run_json(repo.path(), ["show", &id]);

    assert_eq!(
        show.envelope["data"]["task"]["alias"].as_str(),
        Some("original-alias-title")
    );
}

#[test]
fn new_root_ids_are_sequential() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let first = create_task(repo.path(), "First sequential task");
    let second = create_task(repo.path(), "Second sequential task");

    assert_eq!(first, "tsq-1");
    assert_eq!(second, "tsq-2");
}

#[test]
fn child_ids_keep_parent_suffix_shape() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let parent = create_task(repo.path(), "Parent task");
    let child = run_json(repo.path(), ["create", "--parent", &parent, "Child task"]);

    assert_eq!(child.cli.code, 0);
    assert_eq!(
        child.envelope["data"]["task"]["id"].as_str(),
        Some("tsq-1.1")
    );
}

#[test]
fn explicit_sequential_id_is_allowed() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json(
        repo.path(),
        ["create", "Explicit readable", "--id", "tsq-42"],
    );

    assert_eq!(result.cli.code, 0);
    assert_eq!(
        result.envelope["data"]["task"]["id"].as_str(),
        Some("tsq-42")
    );
}

#[test]
fn commands_accept_exact_alias() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let id = create_task(repo.path(), "Improve task search warnings");
    let done = run_json(repo.path(), ["done", "improve-task-search-warnings"]);

    assert_eq!(done.cli.code, 0);
    let show = run_json(repo.path(), ["show", &id]);
    assert_eq!(
        show.envelope["data"]["task"]["status"].as_str(),
        Some("closed")
    );
}

#[test]
fn commands_accept_alias_case_insensitively() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let id = create_task(repo.path(), "Improve task search warnings");
    let done = run_json(repo.path(), ["done", "IMPROVE-TASK-SEARCH-WARNINGS"]);

    assert_eq!(done.cli.code, 0);
    let show = run_json(repo.path(), ["show", &id]);
    assert_eq!(
        show.envelope["data"]["task"]["status"].as_str(),
        Some("closed")
    );
}

#[test]
fn ambiguous_alias_prefix_returns_candidates() {
    let repo = common::make_repo();
    init_repo(repo.path());

    create_task(repo.path(), "Improve search duplicate warnings");
    create_task(repo.path(), "Improve search ranking");

    let result = run_json(repo.path(), ["show", "improve-search"]);

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope["error"]["code"].as_str(),
        Some("TASK_ID_AMBIGUOUS")
    );
    assert!(
        result.envelope["error"]["details"]["candidates"]
            .as_array()
            .expect("candidates")
            .iter()
            .all(|candidate| candidate.get("alias").is_some())
    );
}

#[test]
fn id_prefix_ambiguity_returns_id_and_alias_candidates() {
    let repo = common::make_repo();
    init_repo(repo.path());

    // Both get IDs starting with "tsq-", so prefix "tsq-" is ambiguous.
    create_task(repo.path(), "First task");
    create_task(repo.path(), "Second task");

    let result = run_json(repo.path(), ["show", "tsq-"]);

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope["error"]["code"].as_str(),
        Some("TASK_ID_AMBIGUOUS")
    );
    assert!(
        result.envelope["error"]["details"]["candidates"]
            .as_array()
            .expect("candidates")
            .iter()
            .all(|candidate| candidate.get("id").is_some() && candidate.get("alias").is_some())
    );
}

#[test]
fn sequential_allocation_after_u64_max_returns_error() {
    let repo = common::make_repo();
    init_repo(repo.path());

    // Plant a task at u64::MAX so next allocation would overflow.
    let max_id = "tsq-18446744073709551615";
    let setup = run_json(repo.path(), ["create", "Max ID task", "--id", max_id]);
    assert_eq!(setup.cli.code, 0);

    let result = run_json(repo.path(), ["create", "Should overflow"]);
    assert_eq!(result.cli.code, 2);
    assert_eq!(
        result.envelope["error"]["code"].as_str(),
        Some("ID_OVERFLOW")
    );
}

#[test]
fn search_matches_alias_and_ranks_title_above_notes() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let title_hit = create_task(repo.path(), "Improve task search warnings");
    let notes_hit = create_task(repo.path(), "Unrelated task");
    let note = run_json(
        repo.path(),
        ["note", &notes_hit, "mentions improve task search warnings"],
    );
    assert_eq!(note.cli.code, 0);

    let result = run_json(repo.path(), ["find", "search", "improve search warnings"]);

    assert_eq!(result.cli.code, 0);
    let ids = common::ids_from_task_list(&result.envelope);
    assert_eq!(ids.first(), Some(&title_hit));
    assert!(ids.contains(&notes_hit));
}

#[test]
fn plain_text_search_matches_id_label_and_external_ref() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let id_hit = common::create_task_with_args(repo.path(), "ID-only target", &["--id", "tsq-42"]);
    let label_hit = create_task(repo.path(), "Label-only target");
    let external_ref_hit = common::create_task_with_args(
        repo.path(),
        "External-only target",
        &["--external-ref", "GH-1234"],
    );
    let label = common::label_add(repo.path(), &label_hit, "ops-discovery");
    assert_eq!(label.cli.code, 0);

    let id_result = run_json(repo.path(), ["find", "search", "tsq-42"]);
    let label_result = run_json(repo.path(), ["find", "search", "ops-discovery"]);
    let external_ref_result = run_json(repo.path(), ["find", "search", "GH-1234"]);

    assert_eq!(id_result.cli.code, 0);
    assert_eq!(label_result.cli.code, 0);
    assert_eq!(external_ref_result.cli.code, 0);
    assert!(common::ids_from_task_list(&id_result.envelope).contains(&id_hit));
    assert!(common::ids_from_task_list(&label_result.envelope).contains(&label_hit));
    assert!(common::ids_from_task_list(&external_ref_result.envelope).contains(&external_ref_hit));
}

#[test]
fn find_similar_returns_scores_and_reasons() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let id = create_task(repo.path(), "Improve task search warnings");
    create_task(repo.path(), "Package release automation");

    let result = run_json(repo.path(), ["find", "similar", "task search warning"]);

    assert_eq!(result.cli.code, 0);
    let candidates = result.envelope["data"]["candidates"]
        .as_array()
        .expect("candidates");
    assert_eq!(candidates[0]["task"]["id"].as_str(), Some(id.as_str()));
    assert!(candidates[0]["score"].as_f64().expect("score") >= DEFAULT_SIMILARITY_MIN_SCORE);
    assert!(!candidates[0]["reason"].as_str().expect("reason").is_empty());
}

#[test]
fn find_similar_matches_stable_alias_after_title_edit() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let id = create_task(repo.path(), "Original alias title");
    let edit = run_json(repo.path(), ["edit", &id, "--title", "Renamed title"]);
    assert_eq!(edit.cli.code, 0);

    let result = run_json(repo.path(), ["find", "similar", "original alias title"]);

    assert_eq!(result.cli.code, 0);
    let candidates = result.envelope["data"]["candidates"]
        .as_array()
        .expect("candidates");
    assert_eq!(candidates[0]["task"]["id"].as_str(), Some(id.as_str()));
    assert_eq!(candidates[0]["reason"].as_str(), Some("alias_exact"));
}

#[test]
fn find_similar_human_output_prints_header() {
    let repo = common::make_repo();
    init_repo(repo.path());

    create_task(repo.path(), "Improve task search warnings");

    let result = run_cli(repo.path(), ["find", "similar", "task search warning"]);

    assert_eq!(result.code, 0);
    assert!(result.stdout.lines().next().is_some_and(|line| {
        line.contains("SCORE")
            && line.contains("REASON")
            && line.contains("ID")
            && line.contains("TITLE")
    }));
}

#[test]
fn find_similar_whitespace_query_returns_empty_candidates() {
    let repo = common::make_repo();
    init_repo(repo.path());

    create_task(repo.path(), "Improve task search warnings");

    let result = run_json(repo.path(), ["find", "similar", "   "]);

    assert_eq!(result.cli.code, 0);
    let candidates = result.envelope["data"]["candidates"]
        .as_array()
        .expect("candidates");
    assert!(candidates.is_empty());
}

#[test]
fn search_with_status_filter_still_ranks_title_above_notes() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let title_hit = create_task(repo.path(), "Improve task search warnings");
    let notes_hit = create_task(repo.path(), "Unrelated task");
    let note = run_json(
        repo.path(),
        ["note", &notes_hit, "mentions improve task search warnings"],
    );
    assert_eq!(note.cli.code, 0);

    let result = run_json(
        repo.path(),
        ["find", "search", "status:open improve search warnings"],
    );

    assert_eq!(result.cli.code, 0);
    let ids = common::ids_from_task_list(&result.envelope);
    assert_eq!(ids.first(), Some(&title_hit));
    assert!(ids.contains(&notes_hit));
}

#[test]
fn create_refuses_similar_open_task_without_force() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let existing = create_task(repo.path(), "Improve task search warnings");
    let result = run_json(repo.path(), ["create", "Improve search warning"]);

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope["error"]["code"].as_str(),
        Some("DUPLICATE_TASK_CANDIDATE")
    );
    assert_eq!(
        result.envelope["error"]["details"]["candidates"][0]["id"].as_str(),
        Some(existing.as_str())
    );
}

#[test]
fn create_force_bypasses_duplicate_gate() {
    let repo = common::make_repo();
    init_repo(repo.path());

    create_task(repo.path(), "Improve task search warnings");
    let forced = run_json(repo.path(), ["create", "Improve search warning", "--force"]);

    assert_eq!(forced.cli.code, 0);
    assert_eq!(
        forced.envelope["data"]["task"]["title"].as_str(),
        Some("Improve search warning")
    );
}

#[test]
fn create_rejects_ensure_force_combination() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json(
        repo.path(),
        ["create", "Conflicting flags", "--ensure", "--force"],
    );

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope["error"]["message"].as_str(),
        Some("cannot combine --ensure with --force")
    );
}

#[test]
fn search_hard_filter_only_returns_deterministic_order() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let first = create_task(repo.path(), "Alpha task");
    let second = create_task(repo.path(), "Beta task");
    let third = create_task(repo.path(), "Gamma task");

    let run1 = run_json(repo.path(), ["find", "search", "status:open"]);
    let run2 = run_json(repo.path(), ["find", "search", "status:open"]);

    assert_eq!(run1.cli.code, 0);
    let ids1 = common::ids_from_task_list(&run1.envelope);
    let ids2 = common::ids_from_task_list(&run2.envelope);
    assert_eq!(ids1, ids2, "hard-filter-only search must be deterministic");
    // Sorted by priority (all equal), then created_at, then id
    assert_eq!(ids1, vec![first, second, third]);
}

#[test]
fn multi_create_refuses_incoming_similar_duplicates() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json(
        repo.path(),
        ["create", "Fix search warnings", "Fix search warning"],
    );

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope["error"]["code"].as_str(),
        Some("DUPLICATE_TASK_CANDIDATE")
    );
    // Verify no task was written (all-or-nothing)
    let list = run_json(repo.path(), ["find", "search", "status:open"]);
    assert_eq!(list.cli.code, 0);
    let tasks = list.envelope["data"]["tasks"]
        .as_array()
        .expect("tasks array");
    assert!(tasks.is_empty(), "no tasks should have been created");
    // Verify candidates in error details
    let details = &result.envelope["error"]["details"];
    assert!(details["candidates"].as_array().is_some());
    assert!(!details["candidates"].as_array().unwrap().is_empty());
}

#[test]
fn closed_similar_task_does_not_block_batch_create() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let existing = create_task(repo.path(), "Fix search warnings");
    let done = run_json(repo.path(), ["done", &existing]);
    assert_eq!(done.cli.code, 0);

    let result = run_json(
        repo.path(),
        ["create", "Fix search warning", "Unrelated task"],
    );

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"]
        .as_array()
        .expect("tasks array");
    assert_eq!(tasks.len(), 2);
}

#[test]
fn incoming_duplicate_error_includes_candidates() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json(
        repo.path(),
        [
            "create",
            "Improve search warnings",
            "Improve search warning",
        ],
    );

    assert_eq!(result.cli.code, 1);
    let details = &result.envelope["error"]["details"];
    assert!(details["input_title"].as_str().is_some());
    let candidates = details["candidates"].as_array().expect("candidates array");
    assert!(!candidates.is_empty());
    assert!(candidates[0]["title"].as_str().is_some());
    assert!(candidates[0]["score"].as_f64().is_some());
    assert!(candidates[0]["reason"].as_str().is_some());
}

#[test]
fn ensure_batch_rejects_incoming_similar_duplicates() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json(
        repo.path(),
        [
            "create",
            "--ensure",
            "Improve search warnings",
            "Improve search warning",
        ],
    );

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope["error"]["code"].as_str(),
        Some("DUPLICATE_TASK_CANDIDATE")
    );
    // All-or-nothing: no tasks created
    let list = run_json(repo.path(), ["find", "search", "status:open"]);
    assert_eq!(list.cli.code, 0);
    let tasks = list.envelope["data"]["tasks"]
        .as_array()
        .expect("tasks array");
    assert!(tasks.is_empty(), "no tasks should have been created");
}

#[test]
fn ensure_batch_rejects_existing_similar_task() {
    let repo = common::make_repo();
    init_repo(repo.path());

    // Pre-create a task that is similar but not an exact ensure-reusable match
    create_task(repo.path(), "Improve task search warnings");

    let result = run_json(
        repo.path(),
        [
            "create",
            "--ensure",
            "Improve search warning",
            "Unrelated task",
        ],
    );

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope["error"]["code"].as_str(),
        Some("DUPLICATE_TASK_CANDIDATE")
    );
}

#[test]
fn ensure_batch_allows_exact_reusable_match() {
    let repo = common::make_repo();
    init_repo(repo.path());

    // Pre-create a task that is an exact ensure-reusable match (same normalized title, same parent)
    let existing = create_task(repo.path(), "Improve search warnings");

    let result = run_json(
        repo.path(),
        [
            "create",
            "--ensure",
            "Improve search warnings",
            "Unrelated task",
        ],
    );

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"]
        .as_array()
        .expect("tasks array");
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["id"].as_str(), Some(existing.as_str()));
    assert_ne!(tasks[1]["id"].as_str(), Some(existing.as_str()));
    assert_eq!(tasks[1]["title"].as_str(), Some("Unrelated task"));
}

#[test]
fn ensure_batch_rejects_wrong_parent_exact_match() {
    let repo = common::make_repo();
    init_repo(repo.path());

    // Pre-create a root-level task
    create_task(repo.path(), "Shared child");

    // from-file places "Shared child" under a new parent — different parent
    // from the existing root task.  Ensure must NOT silently exempt this
    // candidate; all-or-nothing should reject before any writes.
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- New parent\n  - Shared child\n").unwrap();

    let result = run_json(
        repo.path(),
        ["create", "--from-file", "tasks.md", "--ensure"],
    );

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope["error"]["code"].as_str(),
        Some("DUPLICATE_TASK_CANDIDATE"),
    );

    // All-or-nothing: "New parent" must NOT have been created either.
    let list = run_json(repo.path(), ["find", "open"]);
    let tasks = list.envelope["data"]["tasks"]
        .as_array()
        .expect("tasks array");
    let has_new_parent = tasks
        .iter()
        .any(|t| t["title"].as_str() == Some("New parent"));
    assert!(
        !has_new_parent,
        "all-or-nothing violated: 'New parent' was created despite rejection"
    );
}

#[test]
fn batch_create_is_atomic_on_later_duplicate_failure() {
    let repo = common::make_repo();
    init_repo(repo.path());

    // Pre-create a task that will collide with the second batch item
    create_task(repo.path(), "Fix search warnings");

    // Batch: first item is unique, second is a duplicate of existing
    let result = run_json(
        repo.path(),
        ["create", "Unique new task", "Fix search warning"],
    );

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope["error"]["code"].as_str(),
        Some("DUPLICATE_TASK_CANDIDATE")
    );

    // Atomicity: "Unique new task" must NOT have been created
    let list = run_json(repo.path(), ["find", "open"]);
    let tasks = list.envelope["data"]["tasks"]
        .as_array()
        .expect("tasks array");
    let has_unique = tasks
        .iter()
        .any(|t| t["title"].as_str() == Some("Unique new task"));
    assert!(
        !has_unique,
        "all-or-nothing violated: 'Unique new task' was created despite batch rejection"
    );
}

#[test]
fn explicit_id_still_checks_duplicate_gate() {
    let repo = common::make_repo();
    init_repo(repo.path());

    create_task(repo.path(), "Improve task search warnings");

    let result = run_json(
        repo.path(),
        ["create", "Improve search warning", "--id", "tsq-99"],
    );

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope["error"]["code"].as_str(),
        Some("DUPLICATE_TASK_CANDIDATE")
    );
}

#[test]
fn human_list_output_shows_alias() {
    let repo = common::make_repo();
    init_repo(repo.path());

    create_task(repo.path(), "Improve task search warnings");
    let result = common::run_cli(repo.path(), ["find", "open"]);

    assert_eq!(result.code, 0);
    assert!(
        result.stdout.contains("ALIAS"),
        "stdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("improve-task-search-warnings"),
        "stdout:\n{}",
        result.stdout
    );
}

#[test]
fn explicit_id_force_bypasses_duplicate_gate() {
    let repo = common::make_repo();
    init_repo(repo.path());

    create_task(repo.path(), "Improve task search warnings");

    let result = run_json(
        repo.path(),
        [
            "create",
            "Improve search warning",
            "--id",
            "tsq-99",
            "--force",
        ],
    );

    assert_eq!(result.cli.code, 0);
    assert_eq!(
        result.envelope["data"]["task"]["id"].as_str(),
        Some("tsq-99")
    );
}
