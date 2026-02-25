mod common;

use common::{make_repo, run_cli};
use serde_json::{Map, Value};
use std::fs;
use std::io::Write;
use std::path::Path;
use tasque::types::{EventRecord, EventType};

fn make_event(id: &str, title: &str) -> EventRecord {
    let mut payload = Map::new();
    payload.insert(
        "title".to_string(),
        Value::String(title.to_string()),
    );
    EventRecord {
        id: Some(id.to_string()),
        event_id: Some(id.to_string()),
        ts: "2026-01-01T00:00:00Z".to_string(),
        actor: "test".to_string(),
        event_type: EventType::TaskCreated,
        task_id: format!("tsq-{}", id),
        payload,
    }
}

fn make_status_event(id: &str, task_id: &str, status: &str) -> EventRecord {
    let mut payload = Map::new();
    payload.insert(
        "status".to_string(),
        Value::String(status.to_string()),
    );
    EventRecord {
        id: Some(id.to_string()),
        event_id: Some(id.to_string()),
        ts: "2026-01-01T00:01:00Z".to_string(),
        actor: "test".to_string(),
        event_type: EventType::TaskStatusSet,
        task_id: task_id.to_string(),
        payload,
    }
}

fn write_jsonl(path: &Path, events: &[EventRecord]) {
    let mut f = fs::File::create(path).unwrap();
    for ev in events {
        writeln!(f, "{}", serde_json::to_string(ev).unwrap()).unwrap();
    }
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    let raw = fs::read_to_string(path).unwrap();
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn test_merge_driver_disjoint_events() {
    let repo = make_repo();
    let dir = repo.path();

    // Base: 3 events
    let base_events = vec![
        make_event("01AAA", "base-1"),
        make_event("01AAB", "base-2"),
        make_event("01AAC", "base-3"),
    ];
    // Ours: base + 2 new
    let mut ours_events = base_events.clone();
    ours_events.push(make_event("01BBB", "ours-1"));
    ours_events.push(make_event("01BBC", "ours-2"));
    // Theirs: base + 2 different new
    let mut theirs_events = base_events.clone();
    theirs_events.push(make_event("01CCC", "theirs-1"));
    theirs_events.push(make_event("01CCD", "theirs-2"));

    let ancestor_path = dir.join("ancestor.jsonl");
    let ours_path = dir.join("ours.jsonl");
    let theirs_path = dir.join("theirs.jsonl");

    write_jsonl(&ancestor_path, &base_events);
    write_jsonl(&ours_path, &ours_events);
    write_jsonl(&theirs_path, &theirs_events);

    let result = run_cli(
        dir,
        [
            "merge-driver",
            ancestor_path.to_str().unwrap(),
            ours_path.to_str().unwrap(),
            theirs_path.to_str().unwrap(),
        ],
    );
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    // Verify the merged file (ours) has all 7 unique events
    let merged = read_jsonl(&ours_path);
    assert_eq!(merged.len(), 7, "Expected 7 merged events, got {}", merged.len());

    // Verify sorted by event ID
    let ids: Vec<&str> = merged
        .iter()
        .map(|v| v.get("id").unwrap().as_str().unwrap())
        .collect();
    let mut sorted_ids = ids.clone();
    sorted_ids.sort();
    assert_eq!(ids, sorted_ids, "Events should be sorted by ID");
}

#[test]
fn test_merge_driver_duplicate_events() {
    let repo = make_repo();
    let dir = repo.path();

    // Base: 3 events
    let base_events = vec![
        make_event("01AAA", "shared-1"),
        make_event("01AAB", "shared-2"),
        make_event("01AAC", "shared-3"),
    ];
    // Ours: base + 2 new, plus duplicate of shared-1
    let mut ours_events = base_events.clone();
    ours_events.push(make_event("01BBB", "ours-1"));
    ours_events.push(make_event("01BBC", "ours-2"));
    // Theirs: base + 2 new, shares 01BBB (same payload = duplicate)
    let mut theirs_events = base_events.clone();
    theirs_events.push(make_event("01BBB", "ours-1")); // same event as ours
    theirs_events.push(make_event("01CCC", "theirs-1"));

    let ancestor_path = dir.join("ancestor.jsonl");
    let ours_path = dir.join("ours.jsonl");
    let theirs_path = dir.join("theirs.jsonl");

    write_jsonl(&ancestor_path, &base_events);
    write_jsonl(&ours_path, &ours_events);
    write_jsonl(&theirs_path, &theirs_events);

    let result = run_cli(
        dir,
        [
            "merge-driver",
            ancestor_path.to_str().unwrap(),
            ours_path.to_str().unwrap(),
            theirs_path.to_str().unwrap(),
        ],
    );
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    // 5 unique: 01AAA, 01AAB, 01AAC, 01BBB, 01BBC, 01CCC = 6 unique
    let merged = read_jsonl(&ours_path);
    assert_eq!(merged.len(), 6, "Expected 6 deduplicated events, got {}", merged.len());

    // stderr should mention duplicates removed
    assert!(
        result.stderr.contains("duplicates removed"),
        "Expected dedup info in stderr: {}",
        result.stderr
    );
}

#[test]
fn test_merge_driver_conflict_on_divergent_payload() {
    let repo = make_repo();
    let dir = repo.path();

    // Base has the event
    let base_events = vec![make_event("01AAA", "original-title")];
    // Ours modifies it (different title for same ID)
    let ours_events = vec![make_event("01AAA", "changed-by-ours")];
    // Theirs also modifies it differently
    let theirs_events = vec![make_event("01AAA", "changed-by-theirs")];

    let ancestor_path = dir.join("ancestor.jsonl");
    let ours_path = dir.join("ours.jsonl");
    let theirs_path = dir.join("theirs.jsonl");

    write_jsonl(&ancestor_path, &base_events);
    write_jsonl(&ours_path, &ours_events);
    write_jsonl(&theirs_path, &theirs_events);

    let result = run_cli(
        dir,
        [
            "merge-driver",
            ancestor_path.to_str().unwrap(),
            ours_path.to_str().unwrap(),
            theirs_path.to_str().unwrap(),
        ],
    );
    assert_eq!(result.code, 1, "Expected conflict exit code 1, got {}", result.code);
    assert!(
        result.stderr.contains("01AAA"),
        "Expected conflicting ID in stderr: {}",
        result.stderr
    );
    assert!(
        result.stderr.contains("MERGE_CONFLICT"),
        "Expected MERGE_CONFLICT in stderr: {}",
        result.stderr
    );
}

#[test]
fn test_merge_driver_empty_ancestor() {
    let repo = make_repo();
    let dir = repo.path();

    let ancestor_path = dir.join("ancestor.jsonl");
    let ours_path = dir.join("ours.jsonl");
    let theirs_path = dir.join("theirs.jsonl");

    // Empty ancestor
    write_jsonl(&ancestor_path, &[]);
    // Ours and theirs each have unique events
    write_jsonl(&ours_path, &[make_event("01AAA", "a"), make_event("01AAB", "b")]);
    write_jsonl(
        &theirs_path,
        &[make_event("01CCC", "c"), make_event("01CCD", "d")],
    );

    let result = run_cli(
        dir,
        [
            "merge-driver",
            ancestor_path.to_str().unwrap(),
            ours_path.to_str().unwrap(),
            theirs_path.to_str().unwrap(),
        ],
    );
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let merged = read_jsonl(&ours_path);
    assert_eq!(merged.len(), 4, "Expected 4 events from empty ancestor merge");

    // Verify sorted
    let ids: Vec<&str> = merged
        .iter()
        .map(|v| v.get("id").unwrap().as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["01AAA", "01AAB", "01CCC", "01CCD"]);
}

#[test]
fn test_merge_driver_mixed_event_types() {
    let repo = make_repo();
    let dir = repo.path();

    let base_events = vec![make_event("01AAA", "task-1")];
    let mut ours_events = base_events.clone();
    ours_events.push(make_status_event("01BBB", "tsq-01AAA", "in_progress"));
    let mut theirs_events = base_events.clone();
    theirs_events.push(make_event("01CCC", "task-2"));

    let ancestor_path = dir.join("ancestor.jsonl");
    let ours_path = dir.join("ours.jsonl");
    let theirs_path = dir.join("theirs.jsonl");

    write_jsonl(&ancestor_path, &base_events);
    write_jsonl(&ours_path, &ours_events);
    write_jsonl(&theirs_path, &theirs_events);

    let result = run_cli(
        dir,
        [
            "merge-driver",
            ancestor_path.to_str().unwrap(),
            ours_path.to_str().unwrap(),
            theirs_path.to_str().unwrap(),
        ],
    );
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let merged = read_jsonl(&ours_path);
    assert_eq!(merged.len(), 3);
}
