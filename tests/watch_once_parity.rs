mod common;

use common::{
    assert_validation_error, create_task, create_task_with_args, ids_from_task_list, init_repo,
    run_cli, run_json,
};
use std::fs::OpenOptions;
use std::io::Write;

#[test]
fn watch_once_json_outputs_expected_frame_with_default_filters() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let task_id = create_task(repo.path(), "Watch default task");

    let result = run_json(repo.path(), ["watch", "--once"]);
    assert_eq!(
        result.cli.code, 0,
        "watch failed\nstdout:\n{}\nstderr:\n{}",
        result.cli.stdout, result.cli.stderr
    );
    assert_eq!(
        result
            .envelope
            .get("command")
            .and_then(|value| value.as_str()),
        Some("tsq watch")
    );

    let data = common::ok_data(&result.envelope);
    assert_eq!(
        data.get("interval_s").and_then(|value| value.as_i64()),
        Some(2)
    );
    assert_eq!(
        data.get("filters")
            .and_then(|value| value.get("status"))
            .and_then(|value| value.as_array())
            .map(|values| values
                .iter()
                .filter_map(|value| value.as_str())
                .collect::<Vec<_>>()),
        Some(vec!["open", "in_progress"])
    );
    assert_eq!(
        data.get("summary")
            .and_then(|value| value.get("total"))
            .and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(ids_from_task_list(&result.envelope), vec![task_id]);
}

#[test]
fn watch_once_json_applies_status_and_assignee_filters() {
    let repo = common::make_repo();
    init_repo(repo.path());

    create_task(repo.path(), "Unfiltered task");
    let target_id = create_task(repo.path(), "Filtered in-progress task");
    let status_update = run_json(repo.path(), ["start", &target_id]);
    assert_eq!(status_update.cli.code, 0);
    let claim_update = run_json(repo.path(), ["claim", &target_id, "--assignee", "alice"]);
    assert_eq!(claim_update.cli.code, 0);

    let result = run_json(
        repo.path(),
        [
            "watch",
            "--once",
            "--status",
            "in_progress",
            "--assignee",
            "alice",
        ],
    );
    assert_eq!(result.cli.code, 0);

    let data = common::ok_data(&result.envelope);
    assert_eq!(ids_from_task_list(&result.envelope), vec![target_id]);
    assert_eq!(
        data.get("filters")
            .and_then(|value| value.get("status"))
            .and_then(|value| value.as_array())
            .map(|values| values
                .iter()
                .filter_map(|value| value.as_str())
                .collect::<Vec<_>>()),
        Some(vec!["in_progress"])
    );
    assert_eq!(
        data.get("filters")
            .and_then(|value| value.get("assignee"))
            .and_then(|value| value.as_str()),
        Some("alice")
    );
}

#[test]
fn watch_once_default_status_filter_excludes_closed_and_canceled_tasks() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let active = create_task(repo.path(), "Still open");
    let closed = create_task(repo.path(), "Will be closed");
    let canceled = create_task(repo.path(), "Will be canceled");

    let close_result = run_json(repo.path(), ["done", &closed]);
    assert_eq!(close_result.cli.code, 0);
    let cancel_result = run_json(repo.path(), ["cancel", &canceled]);
    assert_eq!(cancel_result.cli.code, 0);

    let result = run_json(repo.path(), ["watch", "--once"]);
    assert_eq!(result.cli.code, 0);
    assert_eq!(ids_from_task_list(&result.envelope), vec![active]);
}

#[test]
fn watch_once_human_defaults_to_tree_and_flat_opt_out_keeps_list_view() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let parent = create_task(repo.path(), "Parent task");
    let child = create_task_with_args(repo.path(), "Child task", &["--parent", &parent]);

    let tree = run_cli(repo.path(), ["watch", "--once"]);
    assert_eq!(
        tree.code, 0,
        "watch tree failed\nstdout:\n{}\nstderr:\n{}",
        tree.stdout, tree.stderr
    );
    assert!(
        tree.stdout.contains("└──"),
        "expected default watch output to render tree connectors\nstdout:\n{}",
        tree.stdout
    );
    assert!(
        tree.stdout.contains(&child),
        "expected child in tree output\nstdout:\n{}",
        tree.stdout
    );
    assert!(
        tree.stdout.contains(&parent),
        "expected parent in tree output\nstdout:\n{}",
        tree.stdout
    );

    let flat = run_cli(repo.path(), ["watch", "--once", "--flat"]);
    assert_eq!(flat.code, 0);
    assert!(
        !flat.stdout.contains("└──"),
        "expected --flat output to omit tree connectors\nstdout:\n{}",
        flat.stdout
    );
}

#[test]
fn watch_rejects_tree_and_flat_together() {
    let repo = common::make_repo();
    init_repo(repo.path());
    create_task(repo.path(), "Validation test task");

    let result = run_cli(repo.path(), ["watch", "--once", "--tree", "--flat"]);

    assert_ne!(result.code, 0);
    assert!(
        result.stderr.contains("--flat"),
        "expected conflict error to mention --flat\nstderr:\n{}",
        result.stderr
    );
}

#[test]
fn watch_once_invalid_interval_returns_validation_error_envelope() {
    let repo = common::make_repo();
    init_repo(repo.path());

    create_task_with_args(repo.path(), "Watch validation target", &["-p", "0"]);
    let result = run_json(repo.path(), ["watch", "--once", "--interval", "0"]);

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result
            .envelope
            .get("command")
            .and_then(|value| value.as_str()),
        Some("tsq watch")
    );
    assert_validation_error(&result);
}

#[test]
fn watch_once_invalid_status_csv_returns_validation_error_envelope() {
    let repo = common::make_repo();
    init_repo(repo.path());
    create_task(repo.path(), "Watch invalid status target");

    let result = run_json(repo.path(), ["watch", "--once", "--status", "open,"]);

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result
            .envelope
            .get("command")
            .and_then(|value| value.as_str()),
        Some("tsq watch")
    );
    assert_validation_error(&result);
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|value| value.get("message"))
            .and_then(|value| value.as_str()),
        Some(
            "status must be one of: open, todo, in_progress, blocked, closed, done, canceled, deferred"
        )
    );
}

#[test]
fn watch_once_json_reports_events_corruption_with_error_envelope() {
    let repo = common::make_repo();
    init_repo(repo.path());

    create_task(repo.path(), "Corruption target");

    let events_file = repo.path().join(".tasque").join("events.jsonl");
    let mut handle = OpenOptions::new()
        .append(true)
        .open(&events_file)
        .expect("failed opening events.jsonl for corruption fixture");
    writeln!(handle, "{{").expect("failed writing malformed event line");
    writeln!(handle).expect("failed writing trailing separator line");

    let result = run_json(repo.path(), ["watch", "--once"]);
    assert_eq!(result.cli.code, 2);
    assert_eq!(
        result
            .envelope
            .get("command")
            .and_then(|value| value.as_str()),
        Some("tsq watch")
    );
    assert_eq!(
        result.envelope.get("ok").and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|value| value.get("code"))
            .and_then(|value| value.as_str()),
        Some("EVENTS_CORRUPT")
    );
}
