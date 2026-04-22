use serde_json::json;
use std::fs;
use tasque::store::events::read_events_from_path;
use tempfile::TempDir;

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
