use serde_json::{Map, Value, json};
use tasque::app::repair::scan_orphaned_graph;
use tasque::app::storage::evaluate_task_spec;
use tasque::domain::projector::apply_events;
use tasque::domain::query::{evaluate_query, parse_query};
use tasque::domain::resolve::resolve_task_id;
use tasque::domain::similarity::is_blocking_duplicate;
use tasque::domain::state::create_empty_state;
use tasque::types::{EventRecord, EventType, PlanningState, TaskStatus};

fn event(event_type: EventType, task_id: &str, payload: Value) -> EventRecord {
    EventRecord {
        id: Some(format!("evt-{}", task_id.replace('.', "-"))),
        event_id: None,
        ts: "2026-04-21T00:00:00.000Z".to_string(),
        actor: "test".to_string(),
        event_type,
        task_id: task_id.to_string(),
        payload: payload.as_object().cloned().unwrap_or_else(Map::new),
    }
}

fn created(task_id: &str, payload: Value) -> EventRecord {
    event(EventType::TaskCreated, task_id, payload)
}

fn updated(task_id: &str, payload: Value) -> EventRecord {
    event(EventType::TaskUpdated, task_id, payload)
}

fn assert_invalid_event(events: &[EventRecord]) {
    let err = apply_events(&create_empty_state(), events).expect_err("expected invalid event");
    assert_eq!(err.code, "INVALID_EVENT");
}

#[test]
fn task_created_defaults_optional_typed_fields_when_absent() {
    let state = apply_events(
        &create_empty_state(),
        &[created("tsq-root0001", json!({"title": "root"}))],
    )
    .expect("create should apply");

    let task = state.tasks.get("tsq-root0001").expect("task projected");
    assert_eq!(task.priority, 1);
    assert_eq!(task.status, TaskStatus::Open);
    assert_eq!(task.planning_state, Some(PlanningState::NeedsPlanning));
    assert!(task.labels.is_empty());
}

#[test]
fn task_created_rejects_invalid_optional_typed_fields() {
    for (field, value) in [
        ("kind", json!("bug")),
        ("priority", json!(8)),
        ("status", json!("done")),
        ("planning_state", json!("maybe")),
        ("labels", json!(["ok", 1])),
    ] {
        let mut payload = json!({"title": "bad"}).as_object().cloned().unwrap();
        payload.insert(field.to_string(), value);
        assert_invalid_event(&[created("tsq-bad00001", Value::Object(payload))]);
    }
}

#[test]
fn task_created_rejects_alias_collisions() {
    assert_invalid_event(&[
        created(
            "tsq-root0001",
            json!({"title": "root", "alias": "shared-alias"}),
        ),
        created(
            "tsq-root0002",
            json!({"title": "other", "alias": "shared-alias"}),
        ),
    ]);

    assert_invalid_event(&[created(
        "tsq-root0001",
        json!({"title": "root", "alias": "tsq-root0001"}),
    )]);
}

#[test]
fn task_created_normalizes_explicit_alias() {
    let state = apply_events(
        &create_empty_state(),
        &[created(
            "tsq-root0001",
            json!({"title": "root", "alias": "Mixed Alias!"}),
        )],
    )
    .expect("create should apply");

    let task = state.tasks.get("tsq-root0001").expect("task projected");
    assert_eq!(task.alias, "mixed-alias");
}

#[test]
fn legacy_mixed_case_alias_still_resolves_and_queries() {
    let mut state = apply_events(
        &create_empty_state(),
        &[created(
            "tsq-root0001",
            json!({"title": "root", "alias": "stable-alias"}),
        )],
    )
    .expect("create should apply");
    state
        .tasks
        .get_mut("tsq-root0001")
        .expect("task projected")
        .alias = "Stable-Alias".to_string();

    assert_eq!(
        resolve_task_id(&state, "STABLE-ALIAS", false).expect("exact alias"),
        "tsq-root0001"
    );
    assert_eq!(
        resolve_task_id(&state, "stable", false).expect("alias prefix"),
        "tsq-root0001"
    );

    let filter = parse_query("alias:STABLE-ALIAS").expect("query parses");
    let tasks = state.tasks.values().cloned().collect::<Vec<_>>();
    let matches = evaluate_query(&tasks, &filter, &state);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].id, "tsq-root0001");

    let similar = is_blocking_duplicate("stable alias", &matches[0]).expect("alias similarity");
    assert_eq!(similar.reason, "alias_exact");
}

#[test]
fn task_updated_rejects_invalid_optional_typed_fields() {
    for (field, value) in [
        ("kind", json!("bug")),
        ("priority", json!(8)),
        ("status", json!("done")),
        ("planning_state", json!("maybe")),
        ("labels", json!(["ok", 1])),
    ] {
        let mut payload = Map::new();
        payload.insert(field.to_string(), value);
        assert_invalid_event(&[
            created("tsq-root0001", json!({"title": "root"})),
            updated("tsq-root0001", Value::Object(payload)),
        ]);
    }
}

#[test]
fn task_updated_applies_legacy_status_payload_when_value_is_valid() {
    let state = apply_events(
        &create_empty_state(),
        &[
            created("tsq-root0001", json!({"title": "root"})),
            updated("tsq-root0001", json!({"status": "closed"})),
        ],
    )
    .expect("legacy status patch should apply");

    let task = state.tasks.get("tsq-root0001").expect("task projected");
    assert_eq!(task.status, TaskStatus::Closed);
    assert_eq!(task.closed_at.as_deref(), Some("2026-04-21T00:00:00.000Z"));
}

#[test]
fn task_created_rejects_missing_and_self_direct_refs() {
    for field in ["parent_id", "duplicate_of", "superseded_by", "replies_to"] {
        let mut missing = json!({"title": "bad"}).as_object().cloned().unwrap();
        missing.insert(field.to_string(), json!("tsq-missing1"));
        assert_invalid_event(&[created("tsq-root0001", Value::Object(missing))]);

        let mut self_ref = json!({"title": "bad"}).as_object().cloned().unwrap();
        self_ref.insert(field.to_string(), json!("tsq-root0001"));
        assert_invalid_event(&[created("tsq-root0001", Value::Object(self_ref))]);
    }
}

#[test]
fn task_updated_applies_valid_direct_refs_and_rejects_invalid_direct_refs() {
    let state = apply_events(
        &create_empty_state(),
        &[
            created("tsq-root0001", json!({"title": "root"})),
            created("tsq-target01", json!({"title": "target"})),
            updated(
                "tsq-root0001",
                json!({
                    "parent_id": "tsq-target01",
                    "duplicate_of": "tsq-target01",
                    "superseded_by": "tsq-target01",
                    "replies_to": "tsq-target01"
                }),
            ),
        ],
    )
    .expect("valid refs should apply");
    let task = state.tasks.get("tsq-root0001").expect("task projected");
    assert_eq!(task.parent_id.as_deref(), Some("tsq-target01"));
    assert_eq!(task.duplicate_of.as_deref(), Some("tsq-target01"));
    assert_eq!(task.superseded_by.as_deref(), Some("tsq-target01"));
    assert_eq!(task.replies_to.as_deref(), Some("tsq-target01"));

    for field in ["parent_id", "duplicate_of", "superseded_by", "replies_to"] {
        let mut missing = Map::new();
        missing.insert(field.to_string(), json!("tsq-missing1"));
        assert_invalid_event(&[
            created("tsq-root0001", json!({"title": "root"})),
            updated("tsq-root0001", Value::Object(missing)),
        ]);

        let mut self_ref = Map::new();
        self_ref.insert(field.to_string(), json!("tsq-root0001"));
        assert_invalid_event(&[
            created("tsq-root0001", json!({"title": "root"})),
            updated("tsq-root0001", Value::Object(self_ref)),
        ]);
    }
}

#[test]
fn task_updated_rejects_parent_cycle() {
    assert_invalid_event(&[
        created("tsq-root0001", json!({"title": "root"})),
        created(
            "tsq-child001",
            json!({"title": "child", "parent_id": "tsq-root0001"}),
        ),
        updated("tsq-root0001", json!({"parent_id": "tsq-child001"})),
    ]);
}

#[test]
fn task_spec_attached_rejects_non_canonical_path() {
    for spec_path in [
        "/etc/hosts",
        "../outside/spec.md",
        ".tasque/specs/tsq-other001/spec.md",
    ] {
        assert_invalid_event(&[
            created("tsq-root0001", json!({"title": "root"})),
            event(
                EventType::TaskSpecAttached,
                "tsq-root0001",
                json!({
                    "spec_path": spec_path,
                    "spec_fingerprint": "abc123",
                }),
            ),
        ]);
    }
}

#[test]
fn spec_check_marks_unsafe_metadata_invalid_without_reading_file() {
    let mut state = apply_events(
        &create_empty_state(),
        &[created("tsq-root0001", json!({"title": "root"}))],
    )
    .expect("fixtures should apply");
    let task = state.tasks.get_mut("tsq-root0001").expect("root task");
    task.spec_path = Some("/etc/hosts".to_string());
    task.spec_fingerprint = Some("abc123".to_string());

    let result = evaluate_task_spec("/tmp", "tsq-root0001", task).expect("spec check");

    assert!(!result.ok);
    assert!(!result.spec.attached);
    assert_eq!(result.spec.actual_fingerprint, None);
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == tasque::app::storage::SpecCheckDiagnosticCode::SpecMetadataInvalid
    }));
}

#[test]
fn orphan_scan_reports_invalid_direct_refs_in_projected_state() {
    let mut state = apply_events(
        &create_empty_state(),
        &[
            created("tsq-root0001", json!({"title": "root"})),
            created("tsq-target01", json!({"title": "target"})),
        ],
    )
    .expect("fixtures should apply");

    {
        let root = state.tasks.get_mut("tsq-root0001").expect("root task");
        root.parent_id = Some("tsq-root0001".to_string());
        root.duplicate_of = Some("tsq-missing1".to_string());
        root.superseded_by = Some("tsq-target01".to_string());
        root.replies_to = Some("tsq-missing2".to_string());
    }

    let scan = scan_orphaned_graph(&state);
    let issues: Vec<_> = scan
        .invalid_direct_refs
        .iter()
        .map(|issue| (issue.field, issue.target.as_str(), issue.reason))
        .collect();

    assert_eq!(
        issues,
        vec![
            ("parent_id", "tsq-root0001", "self reference"),
            ("duplicate_of", "tsq-missing1", "target missing"),
            ("replies_to", "tsq-missing2", "target missing"),
        ]
    );
}

#[test]
fn orphan_scan_reports_parent_cycles_in_projected_state() {
    let mut state = apply_events(
        &create_empty_state(),
        &[
            created("tsq-root0001", json!({"title": "root"})),
            created("tsq-child001", json!({"title": "child"})),
        ],
    )
    .expect("fixtures should apply");

    state
        .tasks
        .get_mut("tsq-root0001")
        .expect("root task")
        .parent_id = Some("tsq-child001".to_string());
    state
        .tasks
        .get_mut("tsq-child001")
        .expect("child task")
        .parent_id = Some("tsq-root0001".to_string());

    let scan = scan_orphaned_graph(&state);
    let mut issues: Vec<_> = scan
        .invalid_direct_refs
        .iter()
        .map(|issue| (issue.task_id.as_str(), issue.field, issue.reason))
        .collect();
    issues.sort();

    assert_eq!(
        issues,
        vec![
            ("tsq-child001", "parent_id", "parent cycle"),
            ("tsq-root0001", "parent_id", "parent cycle"),
        ]
    );
}
