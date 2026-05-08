use serde_json::json;
use std::fs;
use tasque::app::state::load_projected_state;
use tasque::domain::events::make_event;
use tasque::store::events::{append_events, read_events_from_path};
use tasque::types::EventType;
use tempfile::TempDir;

fn task_created_event(task_id: &str, title: &str) -> tasque::types::EventRecord {
    let mut payload = serde_json::Map::new();
    payload.insert("title".to_string(), json!(title));
    make_event(
        "test",
        "2026-04-21T00:00:00.000Z",
        EventType::TaskCreated,
        task_id,
        payload,
    )
}

fn write_event(payload: serde_json::Value) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("events.jsonl");
    let event = json!({
        "id": "01HX0000000000000000000001",
        "ts": "2026-04-21T00:00:00.000Z",
        "actor": "test",
        "type": "task.status_set",
        "task_id": "tsq-root0001",
        "payload": payload,
    });
    fs::write(&path, format!("{}\n", event)).expect("write event");
    (dir, path)
}

#[test]
fn read_events_rejects_invalid_status_set_payload_as_corrupt() {
    let (_dir, path) = write_event(json!({"status": "done"}));

    let err = match read_events_from_path(&path) {
        Ok(_) => panic!("invalid status should fail at read boundary"),
        Err(error) => error,
    };

    assert_eq!(err.code, "EVENTS_CORRUPT");
    assert_eq!(err.exit_code, 2);
}

#[test]
fn read_events_accepts_legacy_status_inside_task_updated_payload() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("events.jsonl");
    let event = json!({
        "id": "01HX0000000000000000000002",
        "ts": "2026-04-21T00:00:00.000Z",
        "actor": "test",
        "type": "task.updated",
        "task_id": "tsq-root0001",
        "payload": {"status": "closed"},
    });
    fs::write(&path, format!("{}\n", event)).expect("write event");

    let read = read_events_from_path(&path).expect("valid legacy status patch should read");

    assert_eq!(read.events.len(), 1);
}

#[test]
fn append_trims_malformed_trailing_jsonl_line_before_writing_new_events() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let paths = tasque::store::paths::get_paths(repo);
    std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");

    let first = task_created_event("tsq-root0001", "first");
    let first_line = serde_json::to_string(&first).expect("serialize first");
    fs::write(&paths.events_file, format!("{}\n{{", first_line)).expect("write corrupt tail");

    append_events(repo, &[task_created_event("tsq-root0002", "second")])
        .expect("append after corrupt tail");

    let raw = fs::read_to_string(&paths.events_file).expect("read events");
    assert!(raw.lines().all(|line| line.trim() != "{"));
    let read = read_events_from_path(&paths.events_file).expect("read repaired events");
    let ids = read
        .events
        .iter()
        .map(|event| event.task_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["tsq-root0001", "tsq-root0002"]);
    assert!(read.warning.is_none());
}

#[test]
fn append_trims_malformed_trailing_jsonl_line_even_when_file_ends_with_newline() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let paths = tasque::store::paths::get_paths(repo);
    std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");

    let first = task_created_event("tsq-root0001", "first");
    let first_line = serde_json::to_string(&first).expect("serialize first");
    fs::write(&paths.events_file, format!("{}\n{{\n", first_line)).expect("write corrupt tail");

    append_events(repo, &[task_created_event("tsq-root0002", "second")])
        .expect("append after corrupt tail");

    let raw = fs::read_to_string(&paths.events_file).expect("read events");
    assert!(raw.lines().all(|line| line.trim() != "{"));
    let read = read_events_from_path(&paths.events_file).expect("read repaired events");
    let ids = read
        .events
        .iter()
        .map(|event| event.task_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["tsq-root0001", "tsq-root0002"]);
    assert!(read.warning.is_none());
}

#[test]
fn append_after_malformed_tail_survives_app_level_replay_without_cache() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let paths = tasque::store::paths::get_paths(repo);
    std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");

    let first = task_created_event("tsq-root0001", "first");
    let first_line = serde_json::to_string(&first).expect("serialize first");
    fs::write(&paths.events_file, format!("{}\n{{", first_line)).expect("write corrupt tail");

    append_events(repo, &[task_created_event("tsq-root0002", "second")]).expect("append second");
    let _ = fs::remove_file(&paths.state_file);

    let loaded = load_projected_state(repo).expect("full replay");
    assert!(loaded.state.tasks.contains_key("tsq-root0001"));
    assert!(loaded.state.tasks.contains_key("tsq-root0002"));
}

#[test]
fn append_adds_separator_after_valid_final_line_without_newline() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let paths = tasque::store::paths::get_paths(repo);
    std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");

    let first = task_created_event("tsq-root0001", "first");
    fs::write(
        &paths.events_file,
        serde_json::to_string(&first).expect("serialize first"),
    )
    .expect("write final line without newline");

    append_events(repo, &[task_created_event("tsq-root0002", "second")])
        .expect("append with separator");

    let read = read_events_from_path(&paths.events_file).expect("read events");
    assert_eq!(read.events.len(), 2);
}

#[test]
fn append_rejects_invalid_outbound_event_without_changing_file() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let paths = tasque::store::paths::get_paths(repo);
    append_events(repo, &[task_created_event("tsq-root0001", "first")]).expect("append first");
    let before = fs::read(&paths.events_file).expect("read before");

    let mut bad = task_created_event("tsq-root0002", "second");
    bad.payload.remove("title");

    let err = append_events(repo, &[bad]).expect_err("invalid append should fail");
    assert_eq!(err.code, "EVENT_APPEND_FAILED");
    assert_eq!(fs::read(&paths.events_file).expect("read after"), before);
}

#[test]
fn append_rejects_missing_link_target_missing_supersede_with_and_invalid_priority() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();

    let mut link_payload = serde_json::Map::new();
    link_payload.insert("type".to_string(), json!("relates_to"));
    let missing_target = make_event(
        "test",
        "2026-04-21T00:00:00.000Z",
        EventType::LinkAdded,
        "tsq-root0001",
        link_payload,
    );
    assert_eq!(
        append_events(repo, &[missing_target])
            .expect_err("missing target")
            .code,
        "EVENT_APPEND_FAILED"
    );

    let mut removed_payload = serde_json::Map::new();
    removed_payload.insert("type".to_string(), json!("relates_to"));
    let missing_removed_target = make_event(
        "test",
        "2026-04-21T00:00:00.000Z",
        EventType::LinkRemoved,
        "tsq-root0001",
        removed_payload,
    );
    assert_eq!(
        append_events(repo, &[missing_removed_target])
            .expect_err("missing removed target")
            .code,
        "EVENT_APPEND_FAILED"
    );

    let missing_with = make_event(
        "test",
        "2026-04-21T00:00:00.000Z",
        EventType::TaskSuperseded,
        "tsq-root0001",
        serde_json::Map::new(),
    );
    assert_eq!(
        append_events(repo, &[missing_with])
            .expect_err("missing with")
            .code,
        "EVENT_APPEND_FAILED"
    );

    let mut bad_priority = task_created_event("tsq-root0003", "priority");
    bad_priority
        .payload
        .insert("priority".to_string(), json!(9));
    assert_eq!(
        append_events(repo, &[bad_priority])
            .expect_err("bad priority")
            .code,
        "EVENT_APPEND_FAILED"
    );
}

#[test]
fn append_rejects_empty_required_payload_string() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();

    let empty_title = task_created_event("tsq-root0001", "");

    let err = append_events(repo, &[empty_title]).expect_err("empty title should fail");
    assert_eq!(err.code, "EVENT_APPEND_FAILED");
}

#[test]
fn append_fails_on_malformed_non_final_jsonl_line_without_changing_file() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let paths = tasque::store::paths::get_paths(repo);
    std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");

    let valid_final = serde_json::to_string(&task_created_event("tsq-root0001", "first"))
        .expect("serialize final");
    fs::write(&paths.events_file, format!("{{\n{}\n", valid_final))
        .expect("write malformed earlier line");
    let before = fs::read(&paths.events_file).expect("read before");

    let err = append_events(repo, &[task_created_event("tsq-root0002", "second")])
        .expect_err("malformed earlier line should fail");

    assert_eq!(err.code, "EVENTS_CORRUPT");
    assert_eq!(fs::read(&paths.events_file).expect("read after"), before);
}
