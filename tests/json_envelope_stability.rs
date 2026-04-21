mod common;

use common::{assert_validation_error, create_task, init_repo, run_json};
use tasque::types::SCHEMA_VERSION;

#[test]
fn list_and_search_success_envelopes_keep_schema_and_command_values() {
    let repo = common::make_repo();
    init_repo(repo.path());

    create_task(repo.path(), "Envelope target task");

    let list = run_json(repo.path(), ["list"]);
    assert_eq!(list.cli.code, 0);
    assert_eq!(
        list.envelope
            .get("schema_version")
            .and_then(|value| value.as_u64()),
        Some(SCHEMA_VERSION as u64)
    );
    assert_eq!(
        list.envelope
            .get("command")
            .and_then(|value| value.as_str()),
        Some("tsq list")
    );
    assert_eq!(
        list.envelope.get("ok").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert!(
        list.envelope
            .get("data")
            .and_then(|value| value.get("tasks"))
            .and_then(|value| value.as_array())
            .is_some()
    );

    let search = run_json(repo.path(), ["search", "Envelope"]);
    assert_eq!(search.cli.code, 0);
    assert_eq!(
        search
            .envelope
            .get("schema_version")
            .and_then(|value| value.as_u64()),
        Some(SCHEMA_VERSION as u64)
    );
    assert_eq!(
        search
            .envelope
            .get("command")
            .and_then(|value| value.as_str()),
        Some("tsq search")
    );
    assert_eq!(
        search.envelope.get("ok").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert!(
        search
            .envelope
            .get("data")
            .and_then(|value| value.get("tasks"))
            .and_then(|value| value.as_array())
            .is_some()
    );
}

#[test]
fn list_validation_error_envelope_keeps_stable_shape() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let invalid = run_json(repo.path(), ["list", "--created-after", "not-an-iso"]);
    assert_eq!(invalid.cli.code, 1);
    assert_eq!(
        invalid
            .envelope
            .get("schema_version")
            .and_then(|value| value.as_u64()),
        Some(SCHEMA_VERSION as u64)
    );
    assert_eq!(
        invalid
            .envelope
            .get("command")
            .and_then(|value| value.as_str()),
        Some("tsq list")
    );
    assert_validation_error(&invalid);
}

#[test]
fn list_csv_validation_error_envelope_keeps_stable_shape() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let invalid = run_json(repo.path(), ["list", "--id", ""]);
    assert_eq!(invalid.cli.code, 1);
    assert_eq!(
        invalid
            .envelope
            .get("schema_version")
            .and_then(|value| value.as_u64()),
        Some(SCHEMA_VERSION as u64)
    );
    assert_eq!(
        invalid
            .envelope
            .get("command")
            .and_then(|value| value.as_str()),
        Some("tsq list")
    );
    assert_eq!(
        invalid
            .envelope
            .get("error")
            .and_then(|value| value.get("message"))
            .and_then(|value| value.as_str()),
        Some("--id must not be empty")
    );
    assert_validation_error(&invalid);
}

#[test]
fn update_claim_rejects_planning_flag() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let id = create_task(repo.path(), "Claim planning conflict");
    let invalid = run_json(
        repo.path(),
        ["update", &id, "--claim", "--planning", "planned"],
    );

    assert_eq!(invalid.cli.code, 1);
    assert_eq!(
        invalid
            .envelope
            .get("command")
            .and_then(|value| value.as_str()),
        Some("tsq update")
    );
    assert_eq!(
        invalid
            .envelope
            .get("error")
            .and_then(|value| value.get("message"))
            .and_then(|value| value.as_str()),
        Some(
            "cannot combine --claim with --title/--description/--clear-description/--external-ref/--clear-external-ref/--discovered-from/--clear-discovered-from/--status/--priority/--planning"
        )
    );
    assert_validation_error(&invalid);
}

#[test]
fn update_assignee_requires_claim() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let id = create_task(repo.path(), "Assignee conflict");
    let invalid = run_json(repo.path(), ["update", &id, "--assignee", "alice"]);

    assert_eq!(invalid.cli.code, 1);
    assert_eq!(
        invalid
            .envelope
            .get("command")
            .and_then(|value| value.as_str()),
        Some("tsq update")
    );
    assert_eq!(
        invalid
            .envelope
            .get("error")
            .and_then(|value| value.get("message"))
            .and_then(|value| value.as_str()),
        Some("--assignee requires --claim")
    );
    assert_validation_error(&invalid);
}

#[test]
fn close_and_reopen_require_ids() {
    let repo = common::make_repo();
    init_repo(repo.path());

    for command in ["close", "reopen"] {
        let invalid = run_json(repo.path(), [command]);

        assert_eq!(invalid.cli.code, 1);
        assert_eq!(
            invalid
                .envelope
                .get("command")
                .and_then(|value| value.as_str()),
            Some(format!("tsq {command}").as_str())
        );
        assert_eq!(
            invalid
                .envelope
                .get("error")
                .and_then(|value| value.get("message"))
                .and_then(|value| value.as_str()),
            Some("at least one id is required")
        );
        assert_validation_error(&invalid);
    }
}
