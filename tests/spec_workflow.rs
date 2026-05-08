mod common;

use common::{create_task, init_repo, run_json};
use serde_json::Value;
use std::fs;
use std::path::Path;

#[test]
fn spec_check_before_attach_reports_not_attached() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let task_id = create_task(repo.path(), "Spec check before attach");

    let check = run_json(repo.path(), ["spec", &task_id, "--check"]);

    assert_eq!(check.cli.code, 0);
    assert_eq!(
        check.envelope.get("ok").and_then(Value::as_bool),
        Some(true)
    );
    let data = data(&check.envelope);
    assert_eq!(data.get("ok").and_then(Value::as_bool), Some(false));
    assert_eq!(
        diagnostic_codes(data),
        vec!["SPEC_NOT_ATTACHED".to_string()]
    );
}

#[test]
fn spec_attach_with_complete_required_sections_checks_ok() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let task_id = create_task(repo.path(), "Spec attach complete sections");

    let attach = run_json(repo.path(), ["spec", &task_id, "--text", complete_spec()]);
    assert_eq!(
        attach.cli.code, 0,
        "spec attach failed\nstdout:\n{}\nstderr:\n{}",
        attach.cli.stdout, attach.cli.stderr
    );
    let attach_data = data(&attach.envelope);
    let expected_spec_path = format!(".tasque/specs/{task_id}/spec.md");
    assert_eq!(
        attach_data
            .get("spec")
            .and_then(|value| value.get("spec_path"))
            .and_then(Value::as_str),
        Some(expected_spec_path.as_str())
    );

    let check = run_json(repo.path(), ["spec", &task_id, "--check"]);
    assert_eq!(check.cli.code, 0);
    let check_data = data(&check.envelope);
    assert_eq!(check_data.get("ok").and_then(Value::as_bool), Some(true));
    assert!(diagnostic_codes(check_data).is_empty());
    assert_eq!(
        check_data
            .get("spec")
            .and_then(|value| value.get("missing_sections"))
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0)
    );
}

#[test]
fn spec_check_reports_fingerprint_drift_after_attached_file_edit() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let task_id = create_task(repo.path(), "Spec fingerprint drift");
    let attach = run_json(repo.path(), ["spec", &task_id, "--text", complete_spec()]);
    assert_eq!(attach.cli.code, 0);

    let spec_path = attached_spec_path(repo.path(), data(&attach.envelope));
    fs::write(spec_path, format!("{}\n\nExtra drift.\n", complete_spec())).expect("edit spec");

    let check = run_json(repo.path(), ["spec", &task_id, "--check"]);

    assert_eq!(check.cli.code, 0);
    let check_data = data(&check.envelope);
    assert_eq!(check_data.get("ok").and_then(Value::as_bool), Some(false));
    assert!(
        diagnostic_codes(check_data).contains(&"SPEC_FINGERPRINT_DRIFT".to_string()),
        "expected SPEC_FINGERPRINT_DRIFT\nstdout:\n{}",
        check.cli.stdout
    );
}

#[test]
fn spec_check_reports_missing_required_sections_for_attached_incomplete_spec() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let task_id = create_task(repo.path(), "Spec missing required sections");
    let attach = run_json(repo.path(), ["spec", &task_id, "--text", incomplete_spec()]);
    assert_eq!(attach.cli.code, 0);

    let check = run_json(repo.path(), ["spec", &task_id, "--check"]);

    assert_eq!(check.cli.code, 0);
    let check_data = data(&check.envelope);
    assert_eq!(check_data.get("ok").and_then(Value::as_bool), Some(false));
    assert!(
        diagnostic_codes(check_data).contains(&"SPEC_REQUIRED_SECTIONS_MISSING".to_string()),
        "expected SPEC_REQUIRED_SECTIONS_MISSING\nstdout:\n{}",
        check.cli.stdout
    );
    assert!(
        check_data
            .get("spec")
            .and_then(|value| value.get("missing_sections"))
            .and_then(Value::as_array)
            .is_some_and(|sections| !sections.is_empty()),
        "expected missing sections\nstdout:\n{}",
        check.cli.stdout
    );
}

#[test]
fn update_claim_require_spec_rejects_missing_or_invalid_spec_and_accepts_valid_spec() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let missing_id = create_task(repo.path(), "Claim requires missing spec");
    let missing_claim = run_json(
        repo.path(),
        [
            "claim",
            &missing_id,
            "--assignee",
            "alice",
            "--require-spec",
        ],
    );
    assert_eq!(missing_claim.cli.code, 1);
    assert_eq!(
        error_code(&missing_claim.envelope),
        Some("SPEC_VALIDATION_FAILED")
    );
    assert!(
        error_diagnostic_codes(&missing_claim.envelope).contains(&"SPEC_NOT_ATTACHED".to_string()),
        "expected SPEC_NOT_ATTACHED\nstdout:\n{}",
        missing_claim.cli.stdout
    );

    let invalid_id = create_task(repo.path(), "Claim requires valid spec");
    let attach_invalid = run_json(
        repo.path(),
        ["spec", &invalid_id, "--text", incomplete_spec()],
    );
    assert_eq!(attach_invalid.cli.code, 0);
    let invalid_claim = run_json(
        repo.path(),
        [
            "claim",
            &invalid_id,
            "--assignee",
            "alice",
            "--require-spec",
        ],
    );
    assert_eq!(invalid_claim.cli.code, 1);
    assert_eq!(
        error_code(&invalid_claim.envelope),
        Some("SPEC_VALIDATION_FAILED")
    );
    assert!(
        error_diagnostic_codes(&invalid_claim.envelope)
            .contains(&"SPEC_REQUIRED_SECTIONS_MISSING".to_string()),
        "expected SPEC_REQUIRED_SECTIONS_MISSING\nstdout:\n{}",
        invalid_claim.cli.stdout
    );

    let valid_id = create_task(repo.path(), "Claim requires valid attached spec");
    let attach_valid = run_json(repo.path(), ["spec", &valid_id, "--text", complete_spec()]);
    assert_eq!(attach_valid.cli.code, 0);
    let valid_claim = run_json(
        repo.path(),
        ["claim", &valid_id, "--assignee", "alice", "--require-spec"],
    );

    assert_eq!(
        valid_claim.cli.code, 0,
        "claim with valid spec failed\nstdout:\n{}\nstderr:\n{}",
        valid_claim.cli.stdout, valid_claim.cli.stderr
    );
    assert_eq!(
        valid_claim.envelope.get("ok").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        data(&valid_claim.envelope)
            .get("task")
            .and_then(|value| value.get("assignee"))
            .and_then(Value::as_str),
        Some("alice")
    );
}

fn complete_spec() -> &'static str {
    r#"# Spec

## Overview
Complete direct workflow coverage.

## Constraints / Non-goals
No production behavior changes.

## Interfaces (CLI/API)
Exercise tsq spec <id> --text, tsq spec <id> --check, and tsq claim --require-spec.

## Data model / schema changes
No schema changes.

## Acceptance criteria
Required spec checks pass before claim.

## Test plan
Run cargo test --test spec_workflow --quiet.
"#
}

fn incomplete_spec() -> &'static str {
    r#"# Spec

## Overview
Missing required sections by design.
"#
}

fn data(envelope: &Value) -> &Value {
    envelope.get("data").expect("missing data")
}

fn diagnostic_codes(data: &Value) -> Vec<String> {
    data.get("diagnostics")
        .and_then(Value::as_array)
        .expect("missing diagnostics")
        .iter()
        .filter_map(|diagnostic| diagnostic.get("code").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect()
}

fn error_code(envelope: &Value) -> Option<&str> {
    envelope
        .get("error")
        .and_then(|value| value.get("code"))
        .and_then(Value::as_str)
}

fn error_diagnostic_codes(envelope: &Value) -> Vec<String> {
    envelope
        .get("error")
        .and_then(|value| value.get("details"))
        .and_then(|value| value.get("diagnostics"))
        .and_then(Value::as_array)
        .expect("missing error diagnostics")
        .iter()
        .filter_map(|diagnostic| diagnostic.get("code").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect()
}

fn attached_spec_path(repo: &Path, attach_data: &Value) -> std::path::PathBuf {
    let spec_path = attach_data
        .get("spec")
        .and_then(|value| value.get("spec_path"))
        .and_then(Value::as_str)
        .expect("missing attached spec path");
    repo.join(spec_path)
}
