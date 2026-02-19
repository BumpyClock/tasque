mod common;

use common::{
    assert_validation_error, create_task, ids_from_task_list, init_repo, label_add, run_json,
    run_json_explicit, update_task,
};
use serde_json::Value;

fn assert_error_message(result: &common::JsonOutput, message: &str) {
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|value| value.get("message"))
            .and_then(Value::as_str),
        Some(message)
    );
}

#[test]
fn list_rejects_assignee_and_unassigned_flag_combinations() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let arg_form = run_json(repo.path(), ["list", "--assignee", "alice", "--unassigned"]);
    assert_eq!(arg_form.cli.code, 1);
    assert_validation_error(&arg_form);

    let equals_form = run_json(repo.path(), ["list", "--assignee=alice", "--unassigned"]);
    assert_eq!(equals_form.cli.code, 1);
    assert_validation_error(&equals_form);
}

#[test]
fn list_requires_dep_type_when_dep_direction_is_set() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json(repo.path(), ["list", "--dep-direction", "in"]);
    assert_eq!(result.cli.code, 1);
    assert_validation_error(&result);
}

#[test]
fn list_rejects_empty_only_repeatable_csv_values() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let empty_id = run_json(repo.path(), ["list", "--id", ""]);
    assert_eq!(empty_id.cli.code, 1);
    assert_validation_error(&empty_id);
    assert_error_message(&empty_id, "--id must not be empty");

    let empty_label_any = run_json(repo.path(), ["list", "--label-any", "   "]);
    assert_eq!(empty_label_any.cli.code, 1);
    assert_validation_error(&empty_label_any);
    assert_error_message(&empty_label_any, "--label-any must not be empty");
}

#[test]
fn list_rejects_repeatable_csv_values_with_empty_tokens() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let trailing_id = run_json(repo.path(), ["list", "--id", "tsq-aaaaaaaa,"]);
    assert_eq!(trailing_id.cli.code, 1);
    assert_validation_error(&trailing_id);
    assert_error_message(&trailing_id, "--id values must not be empty");

    let double_comma_label_any = run_json(repo.path(), ["list", "--label-any", "ops,,infra"]);
    assert_eq!(double_comma_label_any.cli.code, 1);
    assert_validation_error(&double_comma_label_any);
    assert_error_message(
        &double_comma_label_any,
        "--label-any values must not be empty",
    );
}

#[test]
fn search_parses_field_prefixed_quoted_terms_as_single_token() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let target = create_task(repo.path(), "my special task");
    let other = create_task(repo.path(), "unrelated work");

    let result = run_json(repo.path(), ["search", r#"title:"my special""#]);
    assert_eq!(result.cli.code, 0);

    let ids = ids_from_task_list(&result.envelope);
    assert!(ids.contains(&target));
    assert!(!ids.contains(&other));
}

#[test]
fn search_rejects_ambiguous_dep_type_field() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json(repo.path(), ["search", "dep_type:blocks"]);
    assert_eq!(result.cli.code, 1);
    assert_validation_error(&result);
}

#[test]
fn search_rejects_invalid_dep_type_direction_value() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json(repo.path(), ["search", "dep_type_in:nope"]);
    assert_eq!(result.cli.code, 1);
    assert_validation_error(&result);
}

#[test]
fn search_handles_negated_filters_with_double_dash_separator() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let open = create_task(repo.path(), "Negation open task");
    let closed = create_task(repo.path(), "Negation closed task");
    let update = update_task(repo.path(), &closed, &["--status", "closed"]);
    assert_eq!(update.cli.code, 0);

    let result = run_json_explicit(repo.path(), ["--json", "search", "--", "-status:closed"]);
    assert_eq!(result.cli.code, 0);

    let ids = ids_from_task_list(&result.envelope);
    assert!(ids.contains(&open));
    assert!(!ids.contains(&closed));
}

#[test]
fn list_json_output_is_stable_for_equivalent_repeatable_csv_inputs() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let first = create_task(repo.path(), "Deterministic A");
    let second = create_task(repo.path(), "Deterministic B");

    let first_label = label_add(repo.path(), &first, "ops");
    assert_eq!(first_label.cli.code, 0);
    let second_label = label_add(repo.path(), &second, "ops");
    assert_eq!(second_label.cli.code, 0);

    let baseline = run_json(
        repo.path(),
        vec![
            "list".to_string(),
            "--id".to_string(),
            format!("{},{}", first, second),
            "--label-any".to_string(),
            "ops".to_string(),
        ],
    );
    let equivalent = run_json(
        repo.path(),
        vec![
            "list".to_string(),
            "--id".to_string(),
            second.clone(),
            "--id".to_string(),
            first.clone(),
            "--label-any".to_string(),
            "ops,ops".to_string(),
        ],
    );

    assert_eq!(baseline.cli.code, 0);
    assert_eq!(equivalent.cli.code, 0);
    assert_eq!(baseline.cli.stdout, equivalent.cli.stdout);
}
