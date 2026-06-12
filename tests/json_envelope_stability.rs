mod common;

use common::{assert_validation_error, create_task, init_repo, run_json};
use serde_json::Value;
use tasque::types::SCHEMA_VERSION;

#[test]
fn list_and_search_success_envelopes_keep_schema_and_command_values() {
    let repo = common::make_repo();
    init_repo(repo.path());

    create_task(repo.path(), "Envelope target task");

    let list = run_json(repo.path(), ["find", "open"]);
    assert_success_tasks(&list, "tsq find open");

    let search = run_json(repo.path(), ["find", "search", "Envelope"]);
    assert_success_tasks(&search, "tsq find search");
}

#[test]
fn list_validation_error_envelope_keeps_stable_shape() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let invalid = run_json(
        repo.path(),
        ["find", "open", "--created-after", "not-an-iso"],
    );
    assert_error_command(&invalid, "tsq find open");
}

#[test]
fn list_csv_validation_error_envelope_keeps_stable_shape() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let invalid = run_json(repo.path(), ["find", "open", "--id", ""]);
    assert_error_command(&invalid, "tsq find open");
    assert_eq!(
        invalid
            .envelope
            .get("error")
            .and_then(|value| value.get("message"))
            .and_then(Value::as_str),
        Some("--id must not be empty")
    );
}

fn assert_success_tasks(result: &common::JsonOutput, command: &str) {
    assert_eq!(result.cli.code, 0);
    assert_common_envelope(result, command);
    assert_eq!(
        result.envelope.get("ok").and_then(Value::as_bool),
        Some(true)
    );
    assert!(
        result
            .envelope
            .get("data")
            .and_then(|value| value.get("tasks"))
            .and_then(Value::as_array)
            .is_some()
    );
}

fn assert_error_command(result: &common::JsonOutput, command: &str) {
    assert_eq!(result.cli.code, 1);
    assert_common_envelope(result, command);
    assert_validation_error(result);
}

fn assert_common_envelope(result: &common::JsonOutput, command: &str) {
    assert_eq!(
        result
            .envelope
            .get("schema_version")
            .and_then(Value::as_u64),
        Some(SCHEMA_VERSION as u64)
    );
    assert_eq!(
        result.envelope.get("command").and_then(Value::as_str),
        Some(command)
    );
}
