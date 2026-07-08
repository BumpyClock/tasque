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
    assert_eq!(
        merged_ids(&run.ours_path),
        vec!["01AAA", "01AAB", "01AAC", "01BBB", "01BBC", "01CCC"]
    );
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
fn test_merge_driver_commutative_independent_creates() {
    // Both sides add independent root task creates. A<-B and B<-A must produce
    // byte-identical merged output.
    let repo_left = make_repo();
    let repo_right = make_repo();

    let ours_events = vec![
        make_event("01AAA", "ours-task"),
        make_event("01AAB", "ours-task-2"),
    ];
    let theirs_events = vec![
        make_event("01BBB", "theirs-task"),
        make_event("01BBC", "theirs-task-2"),
    ];

    let run_left = run_merge_driver(repo_left.path(), &[], &ours_events, &theirs_events);
    let run_right = run_merge_driver(repo_right.path(), &[], &theirs_events, &ours_events);

    assert_eq!(
        run_left.result.code, 0,
        "stderr: {}",
        run_left.result.stderr
    );
    assert_eq!(
        run_right.result.code, 0,
        "stderr: {}",
        run_right.result.stderr
    );

    let left_out = fs::read_to_string(&run_left.ours_path).unwrap();
    let right_out = fs::read_to_string(&run_right.ours_path).unwrap();
    assert_eq!(
        left_out, right_out,
        "merged output must be byte-identical regardless of merge direction"
    );
    assert_eq!(
        merged_ids(&run_left.ours_path),
        vec!["01AAA", "01AAB", "01BBB", "01BBC"]
    );
}

#[test]
fn test_merge_driver_event_id_fallback_dedup() {
    // Legacy records carrying only `event_id` (no `id`) still dedup correctly
    // against a peer carrying `id` with the same value.
    let repo = make_repo();
    let mut payload = Map::new();
    payload.insert("title".to_string(), Value::String("shared".to_string()));
    let legacy_only_event_id = EventRecord {
        id: None,
        event_id: Some("01SHARED".to_string()),
        ts: "2026-01-01T00:00:00Z".to_string(),
        actor: "test".to_string(),
        event_type: EventType::TaskCreated,
        task_id: "tsq-01SHARED".to_string(),
        payload: payload.clone(),
    };
    let canonical = EventRecord {
        id: Some("01SHARED".to_string()),
        event_id: Some("01SHARED".to_string()),
        ..legacy_only_event_id.clone()
    };

    let run = run_merge_driver(repo.path(), &[], &[legacy_only_event_id], &[canonical]);

    assert_eq!(run.result.code, 0, "stderr: {}", run.result.stderr);
    assert_eq!(read_jsonl(&run.ours_path).len(), 1);
}

#[test]
fn test_merge_driver_duplicate_task_id_yields_replay_failure() {
    // Two sides independently create events with distinct event ids but the
    // SAME task_id. The merged stream is causally invalid (TASK_EXISTS), so the
    // merge driver must surface MERGE_REPLAY_FAILED instead of writing a bad file.
    let repo = make_repo();
    let ours_event = make_event("01OURS", "ours-title");
    let mut theirs_event = make_event("01THRS", "theirs-title");
    // Collision: same task_id as ours, different event id.
    theirs_event.task_id = ours_event.task_id.clone();

    let run = run_merge_driver(repo.path(), &[], &[ours_event], &[theirs_event]);

    assert_eq!(run.result.code, 2, "stderr: {}", run.result.stderr);
    assert!(
        run.result.stderr.contains("MERGE_REPLAY_FAILED"),
        "stderr: {}",
        run.result.stderr
    );
    // Replay failure must not have overwritten the working file with the bad
    // merged stream; it should still hold only the original ours input.
    assert_eq!(read_jsonl(&run.ours_path).len(), 1);
    assert_eq!(merged_ids(&run.ours_path), vec!["01OURS"]);
}

#[test]
fn test_merge_driver_cycle_fallback_is_deterministic() {
    // If two sources contain the same independent events in opposite order,
    // source-order constraints form a cycle. The fallback must stay stable and
    // direction-independent instead of producing merge-order drift.
    let repo_left = make_repo();
    let repo_right = make_repo();
    let first = make_event("01AAA", "first");
    let second = make_event("01BBB", "second");

    let ours = vec![second.clone(), first.clone()];
    let theirs = vec![first, second];

    let run_left = run_merge_driver(repo_left.path(), &[], &ours, &theirs);
    let run_right = run_merge_driver(repo_right.path(), &[], &theirs, &ours);

    assert_eq!(
        run_left.result.code, 0,
        "stderr: {}",
        run_left.result.stderr
    );
    assert_eq!(
        run_right.result.code, 0,
        "stderr: {}",
        run_right.result.stderr
    );
    assert_eq!(
        fs::read_to_string(&run_left.ours_path).unwrap(),
        fs::read_to_string(&run_right.ours_path).unwrap(),
        "cycle fallback must be byte-identical regardless of merge direction"
    );
    assert_eq!(merged_ids(&run_left.ours_path), vec!["01AAA", "01BBB"]);
}

#[test]
fn test_merge_driver_both_sides_add_identical_dedup() {
    // Both sides add the SAME new event (same id, same payload). It must appear
    // exactly once in the merged output.
    let repo = make_repo();
    let base_events = vec![make_event("01BASE", "base")];
    let shared_new = make_event("01NEW", "new-from-both");
    let mut ours_events = base_events.clone();
    ours_events.push(shared_new.clone());
    let mut theirs_events = base_events.clone();
    theirs_events.push(shared_new);

    let run = run_merge_driver(repo.path(), &base_events, &ours_events, &theirs_events);

    assert_eq!(run.result.code, 0, "stderr: {}", run.result.stderr);
    assert_eq!(read_jsonl(&run.ours_path).len(), 2);
    assert_eq!(merged_ids(&run.ours_path), vec!["01BASE", "01NEW"]);
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
