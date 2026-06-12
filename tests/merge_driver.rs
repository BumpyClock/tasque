mod common;

use common::{CliOutput, make_repo, run_cli};
use serde_json::{Map, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tasque::types::{EventRecord, EventType};

struct MergeRun {
    result: CliOutput,
    ours_path: PathBuf,
}

fn make_event(id: &str, title: &str) -> EventRecord {
    let mut payload = Map::new();
    payload.insert("title".to_string(), Value::String(title.to_string()));
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
    payload.insert("status".to_string(), Value::String(status.to_string()));
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

fn make_update_event(id: &str, task_id: &str, title: &str) -> EventRecord {
    let mut payload = Map::new();
    payload.insert("title".to_string(), Value::String(title.to_string()));
    EventRecord {
        id: Some(id.to_string()),
        event_id: Some(id.to_string()),
        ts: "2026-01-01T00:02:00Z".to_string(),
        actor: "test".to_string(),
        event_type: EventType::TaskUpdated,
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

fn run_merge_driver(
    dir: &Path,
    ancestor_events: &[EventRecord],
    ours_events: &[EventRecord],
    theirs_events: &[EventRecord],
) -> MergeRun {
    let ancestor_path = dir.join("ancestor.jsonl");
    let ours_path = dir.join("ours.jsonl");
    let theirs_path = dir.join("theirs.jsonl");
    write_jsonl(&ancestor_path, ancestor_events);
    write_jsonl(&ours_path, ours_events);
    write_jsonl(&theirs_path, theirs_events);
    let result = run_cli(
        dir,
        [
            "merge-driver",
            ancestor_path.to_str().unwrap(),
            ours_path.to_str().unwrap(),
            theirs_path.to_str().unwrap(),
        ],
    );
    MergeRun { result, ours_path }
}

fn merged_ids(path: &Path) -> Vec<String> {
    read_jsonl(path)
        .iter()
        .map(|v| v.get("id").unwrap().as_str().unwrap().to_string())
        .collect()
}

#[test]
fn test_merge_driver_disjoint_events() {
    let repo = make_repo();
    let base_events = vec![
        make_event("01AAA", "base-1"),
        make_event("01AAB", "base-2"),
        make_event("01AAC", "base-3"),
    ];
    let mut ours_events = base_events.clone();
    ours_events.push(make_event("01BBB", "ours-1"));
    ours_events.push(make_event("01BBC", "ours-2"));
    let mut theirs_events = base_events.clone();
    theirs_events.push(make_event("01CCC", "theirs-1"));
    theirs_events.push(make_event("01CCD", "theirs-2"));

    let run = run_merge_driver(repo.path(), &base_events, &ours_events, &theirs_events);

    assert_eq!(run.result.code, 0, "stderr: {}", run.result.stderr);
    assert_eq!(read_jsonl(&run.ours_path).len(), 7);
    assert_eq!(
        merged_ids(&run.ours_path),
        vec![
            "01AAA", "01AAB", "01AAC", "01BBB", "01BBC", "01CCC", "01CCD"
        ]
    );
}

#[test]
fn test_merge_driver_duplicate_events() {
    let repo = make_repo();
    let base_events = vec![
        make_event("01AAA", "shared-1"),
        make_event("01AAB", "shared-2"),
        make_event("01AAC", "shared-3"),
    ];
    let mut ours_events = base_events.clone();
    ours_events.push(make_event("01BBB", "ours-1"));
    ours_events.push(make_event("01BBC", "ours-2"));
    let mut theirs_events = base_events.clone();
    theirs_events.push(make_event("01BBB", "ours-1"));
    theirs_events.push(make_event("01CCC", "theirs-1"));

    let run = run_merge_driver(repo.path(), &base_events, &ours_events, &theirs_events);

    assert_eq!(run.result.code, 0, "stderr: {}", run.result.stderr);
    assert_eq!(read_jsonl(&run.ours_path).len(), 6);
    assert!(run.result.stderr.contains("duplicates removed"));
}

#[test]
fn test_merge_driver_conflict_on_divergent_payload() {
    let repo = make_repo();
    let base_events = vec![make_event("01AAA", "original-title")];
    let ours_events = vec![make_event("01AAA", "changed-by-ours")];
    let theirs_events = vec![make_event("01AAA", "changed-by-theirs")];

    let run = run_merge_driver(repo.path(), &base_events, &ours_events, &theirs_events);

    assert_eq!(run.result.code, 1);
    assert!(run.result.stderr.contains("01AAA"));
    assert!(run.result.stderr.contains("MERGE_CONFLICT"));
}

#[test]
fn test_merge_driver_empty_ancestor() {
    let repo = make_repo();
    let ours_events = vec![make_event("01AAA", "a"), make_event("01AAB", "b")];
    let theirs_events = vec![make_event("01CCC", "c"), make_event("01CCD", "d")];

    let run = run_merge_driver(repo.path(), &[], &ours_events, &theirs_events);

    assert_eq!(run.result.code, 0, "stderr: {}", run.result.stderr);
    assert_eq!(read_jsonl(&run.ours_path).len(), 4);
    assert_eq!(
        merged_ids(&run.ours_path),
        vec!["01AAA", "01AAB", "01CCC", "01CCD"]
    );
}

#[test]
fn test_merge_driver_preserves_causal_source_order_when_ids_sort_backwards() {
    let repo = make_repo();
    let create = make_event("02CREATE", "original");
    let update = make_update_event("01UPDATE", "tsq-02CREATE", "renamed");
    let theirs = make_event("03THEIRS", "theirs");

    let run = run_merge_driver(repo.path(), &[], &[create, update], &[theirs]);

    assert_eq!(run.result.code, 0, "stderr: {}", run.result.stderr);
    assert_eq!(
        merged_ids(&run.ours_path),
        vec!["02CREATE", "01UPDATE", "03THEIRS"]
    );
}

#[test]
fn test_merge_driver_mixed_event_types() {
    let repo = make_repo();
    let base_events = vec![make_event("01AAA", "task-1")];
    let mut ours_events = base_events.clone();
    ours_events.push(make_status_event("01BBB", "tsq-01AAA", "in_progress"));
    let mut theirs_events = base_events.clone();
    theirs_events.push(make_event("01CCC", "task-2"));

    let run = run_merge_driver(repo.path(), &base_events, &ours_events, &theirs_events);

    assert_eq!(run.result.code, 0, "stderr: {}", run.result.stderr);
    assert_eq!(read_jsonl(&run.ours_path).len(), 3);
}
