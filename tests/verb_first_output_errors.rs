mod common;

use common::{create_task, init_repo, run_cli, run_json_explicit};
use serde_json::Value;

#[test]
fn format_json_outputs_json_envelope() {
    let repo = common::make_repo();
    init_repo(repo.path());
    create_task(repo.path(), "Format json target");

    let result = run_json_explicit(repo.path(), ["--format", "json", "find", "open"]);

    assert_eq!(result.cli.code, 0);
    assert_eq!(
        result.envelope.get("ok").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        result.envelope.get("command").and_then(Value::as_str),
        Some("tsq find open")
    );
}

#[test]
fn json_conflicts_with_format_human() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json_explicit(repo.path(), ["--json", "--format", "human", "find", "open"]);

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope.get("ok").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(Value::as_str),
        Some("VALIDATION_ERROR")
    );
}

#[test]
fn parse_errors_use_json_envelope_when_json_requested() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json_explicit(repo.path(), ["--json", "find", "open", "--bogus"]);

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope.get("ok").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(Value::as_str),
        Some("VALIDATION_ERROR")
    );
    assert!(
        result.cli.stderr.trim().is_empty(),
        "stderr:\n{}",
        result.cli.stderr
    );
}

#[test]
fn parse_errors_use_json_envelope_when_format_json_requested_late() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json_explicit(repo.path(), ["find", "open", "--bogus", "--format", "json"]);

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope.get("ok").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(Value::as_str),
        Some("VALIDATION_ERROR")
    );
    assert!(
        result.cli.stderr.trim().is_empty(),
        "stderr:\n{}",
        result.cli.stderr
    );
}

#[test]
fn parse_errors_use_json_envelope_when_format_json_equals_requested() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json_explicit(repo.path(), ["find", "open", "--bogus", "--format=json"]);

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope.get("ok").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(Value::as_str),
        Some("VALIDATION_ERROR")
    );
    assert!(
        result.cli.stderr.trim().is_empty(),
        "stderr:\n{}",
        result.cli.stderr
    );
}

#[test]
fn root_resolution_errors_use_json_envelope_when_format_json_requested() {
    let repo = common::make_repo();
    std::fs::create_dir_all(repo.path().join(".tasque")).unwrap();
    std::fs::write(
        repo.path().join(".tasque/config.json"),
        r#"{"schema_version":1,"snapshot_every":200,"sync_branch":"tsq-sync"}"#,
    )
    .unwrap();

    let result = run_json_explicit(repo.path(), ["--format", "json", "find", "open"]);

    assert_eq!(result.cli.code, 2);
    assert_eq!(
        result.envelope.get("ok").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(Value::as_str),
        Some("GIT_NOT_AVAILABLE")
    );
    assert!(
        result.cli.stderr.trim().is_empty(),
        "stderr:\n{}",
        result.cli.stderr
    );
}

#[test]
fn root_resolution_errors_use_json_envelope_when_format_json_equals_requested() {
    let repo = common::make_repo();
    std::fs::create_dir_all(repo.path().join(".tasque")).unwrap();
    std::fs::write(
        repo.path().join(".tasque/config.json"),
        r#"{"schema_version":1,"snapshot_every":200,"sync_branch":"tsq-sync"}"#,
    )
    .unwrap();

    let result = run_json_explicit(repo.path(), ["--format=json", "find", "open"]);

    assert_eq!(result.cli.code, 2);
    assert_eq!(
        result.envelope.get("ok").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(Value::as_str),
        Some("GIT_NOT_AVAILABLE")
    );
    assert!(
        result.cli.stderr.trim().is_empty(),
        "stderr:\n{}",
        result.cli.stderr
    );
}

#[test]
fn format_json_help_prints_clap_help_not_error_envelope() {
    let repo = common::make_repo();

    let result = run_cli(repo.path(), ["--format", "json", "--help"]);

    assert_eq!(result.code, 0);
    assert!(
        result.stdout.contains("Usage:"),
        "stdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Examples:"),
        "stdout:\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains(r#""ok": false"#),
        "stdout:\n{}",
        result.stdout
    );
    assert!(
        result.stderr.trim().is_empty(),
        "stderr:\n{}",
        result.stderr
    );
}

#[test]
fn format_json_version_prints_clap_version_not_error_envelope() {
    let repo = common::make_repo();

    let result = run_cli(repo.path(), ["--format", "json", "--version"]);

    assert_eq!(result.code, 0);
    assert!(
        result.stdout.starts_with("tsq "),
        "stdout:\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains(r#""ok": false"#),
        "stdout:\n{}",
        result.stdout
    );
    assert!(
        result.stderr.trim().is_empty(),
        "stderr:\n{}",
        result.stderr
    );
}

#[test]
fn root_help_contains_canonical_examples() {
    let repo = common::make_repo();

    let result = run_cli(repo.path(), ["--help"]);

    assert_eq!(result.code, 0);
    assert!(
        result.stdout.contains("Examples:"),
        "stdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("tsq create \"Fix auth redirect\""),
        "stdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("tsq find ready --lane coding"),
        "stdout:\n{}",
        result.stdout
    );
}

#[test]
fn key_verb_help_contains_examples() {
    let repo = common::make_repo();

    for (args, expected) in [
        (
            vec!["create", "--help"],
            vec![
                "Examples:",
                "tsq create --from-file tasks.md",
                "tasks.md format:",
            ],
        ),
        (
            vec!["note", "--help"],
            vec![
                "Examples:",
                "tsq note tsq-abc12345",
                "tsq notes tsq-abc12345",
            ],
        ),
        (
            vec!["spec", "--help"],
            vec!["Examples:", "tsq spec tsq-abc12345 --file docs/spec.md"],
        ),
        (
            vec!["find", "--help"],
            vec!["Examples:", "tsq find ready --lane planning"],
        ),
        (
            vec!["done", "--help"],
            vec!["Examples:", "tsq done tsq-abc12345 --note \"merged\""],
        ),
    ] {
        let result = run_cli(repo.path(), args);
        assert_eq!(result.code, 0);
        for needle in expected {
            assert!(
                result.stdout.contains(needle),
                "missing `{needle}`\nstdout:\n{}",
                result.stdout
            );
        }
    }
}

#[test]
fn removed_note_add_points_to_note_command() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_cli(repo.path(), ["note", "add", "tsq-aaaaaaaa", "text"]);

    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("tsq note <id> \"text\""),
        "stderr:\n{}",
        result.stderr
    );
}
