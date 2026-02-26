use crate::errors::TsqError;
use crate::store::events::read_events_from_path;
use crate::types::{EventRecord, MergeDriverOutcome};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

/// Extract the canonical event ID from an EventRecord.
/// Prefers `id`, falls back to `event_id`.
fn event_id(record: &EventRecord) -> Option<&str> {
    record.id.as_deref().or(record.event_id.as_deref())
}

/// Serialize an EventRecord to its canonical JSON string for comparison.
/// We use serde_json's deterministic serialization (keys in struct order).
fn canonical_json(record: &EventRecord) -> Result<String, TsqError> {
    serde_json::to_string(record).map_err(|e| {
        TsqError::new(
            "MERGE_SERIALIZE_FAILED",
            format!("Failed serializing event for merge comparison: {}", e),
            2,
        )
    })
}

/// Merge three versions of an events.jsonl file (ancestor, ours, theirs).
///
/// Algorithm:
/// 1. Read all three files
/// 2. Build a map keyed by event ID
/// 3. Detect conflicts: same ID but different payload across files
/// 4. Deduplicate identical events
/// 5. Sort by event ID (ULIDs are lexicographically time-ordered)
/// 6. Write merged result to `ours` (git merge convention: result goes to %A)
pub fn merge_events_files(
    ancestor: &Path,
    ours: &Path,
    theirs: &Path,
) -> Result<MergeDriverOutcome, TsqError> {
    let ancestor_result = read_events_from_path(ancestor)?;
    let ours_result = read_events_from_path(ours)?;
    let theirs_result = read_events_from_path(theirs)?;

    // Map: event_id -> (canonical_json, EventRecord)
    let mut seen: HashMap<String, (String, EventRecord)> = HashMap::new();
    let mut conflicting_ids: Vec<String> = Vec::new();
    let mut total_input = 0usize;

    let all_sources = [
        ancestor_result.events,
        ours_result.events,
        theirs_result.events,
    ];

    for events in &all_sources {
        for record in events {
            total_input += 1;
            let id = match event_id(record) {
                Some(id) => id.to_string(),
                None => {
                    return Err(TsqError::new(
                        "MERGE_MISSING_ID",
                        "Event missing id field during merge",
                        2,
                    ));
                }
            };

            let json = canonical_json(record)?;

            match seen.get(&id) {
                Some((existing_json, _)) => {
                    if *existing_json != json && !conflicting_ids.contains(&id) {
                        conflicting_ids.push(id.clone());
                    }
                    // Same ID + same payload = duplicate, skip
                }
                None => {
                    seen.insert(id, (json, record.clone()));
                }
            }
        }
    }

    conflicting_ids.sort();

    if !conflicting_ids.is_empty() {
        return Ok(MergeDriverOutcome {
            total_events: seen.len(),
            duplicates_removed: total_input.saturating_sub(seen.len()),
            conflict: true,
            conflicting_ids,
        });
    }

    // Sort by event ID (ULID lexicographic = chronological)
    let mut merged: Vec<(String, EventRecord)> = seen
        .into_iter()
        .map(|(id, (_, record))| (id, record))
        .collect();
    merged.sort_by(|(a, _), (b, _)| a.cmp(b));

    let duplicates_removed = total_input.saturating_sub(merged.len());
    let total_events = merged.len();

    // Write merged result to ours path (git expects result at %A)
    write_events_to_path(ours, &merged)?;

    Ok(MergeDriverOutcome {
        total_events,
        duplicates_removed,
        conflict: false,
        conflicting_ids: Vec::new(),
    })
}

/// Write a sorted list of events to a file as JSONL.
fn write_events_to_path(path: &Path, events: &[(String, EventRecord)]) -> Result<(), TsqError> {
    let mut file = fs::File::create(path).map_err(|e| {
        TsqError::new(
            "MERGE_WRITE_FAILED",
            format!("Failed writing merged events to {}: {}", path.display(), e),
            2,
        )
    })?;

    for (_, record) in events {
        let line = serde_json::to_string(record).map_err(|e| {
            TsqError::new(
                "MERGE_SERIALIZE_FAILED",
                format!("Failed serializing merged event: {}", e),
                2,
            )
        })?;
        writeln!(file, "{}", line).map_err(|e| {
            TsqError::new(
                "MERGE_WRITE_FAILED",
                format!("Failed writing merged event line: {}", e),
                2,
            )
        })?;
    }

    file.sync_all().map_err(|e| {
        TsqError::new(
            "MERGE_WRITE_FAILED",
            format!("Failed syncing merged events file: {}", e),
            2,
        )
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EventType;
    use serde_json::Map;
    use tempfile::TempDir;

    fn make_event(id: &str, title: &str) -> EventRecord {
        let mut payload = Map::new();
        payload.insert(
            "title".to_string(),
            serde_json::Value::String(title.to_string()),
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

    fn write_events(dir: &Path, name: &str, events: &[EventRecord]) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        for ev in events {
            writeln!(f, "{}", serde_json::to_string(ev).unwrap()).unwrap();
        }
        path
    }

    #[test]
    fn test_disjoint_merge() {
        let tmp = TempDir::new().unwrap();
        let base = vec![make_event("01A", "base1")];
        let ours_events = vec![make_event("01A", "base1"), make_event("01B", "ours1")];
        let theirs_events = vec![make_event("01A", "base1"), make_event("01C", "theirs1")];

        let ancestor = write_events(tmp.path(), "ancestor.jsonl", &base);
        let ours = write_events(tmp.path(), "ours.jsonl", &ours_events);
        let theirs = write_events(tmp.path(), "theirs.jsonl", &theirs_events);

        let result = merge_events_files(&ancestor, &ours, &theirs).unwrap();
        assert!(!result.conflict);
        assert_eq!(result.total_events, 3);
        assert!(result.duplicates_removed > 0);

        // Verify merged file content
        let merged = read_events_from_path(&ours).unwrap();
        assert_eq!(merged.events.len(), 3);
        // Should be sorted by ID
        let ids: Vec<&str> = merged
            .events
            .iter()
            .map(|e| e.id.as_deref().unwrap())
            .collect();
        assert_eq!(ids, vec!["01A", "01B", "01C"]);
    }

    #[test]
    fn test_duplicate_dedup() {
        let tmp = TempDir::new().unwrap();
        let ev = make_event("01A", "same");
        let base = vec![ev.clone()];
        let ours_events = vec![ev.clone(), make_event("01B", "new")];
        let theirs_events = vec![ev.clone()];

        let ancestor = write_events(tmp.path(), "ancestor.jsonl", &base);
        let ours = write_events(tmp.path(), "ours.jsonl", &ours_events);
        let theirs = write_events(tmp.path(), "theirs.jsonl", &theirs_events);

        let result = merge_events_files(&ancestor, &ours, &theirs).unwrap();
        assert!(!result.conflict);
        assert_eq!(result.total_events, 2); // 01A + 01B
    }

    #[test]
    fn test_conflict_on_divergent_payload() {
        let tmp = TempDir::new().unwrap();
        let base = vec![make_event("01A", "original")];
        let ours_events = vec![make_event("01A", "changed-ours")];
        let theirs_events = vec![make_event("01A", "changed-theirs")];

        let ancestor = write_events(tmp.path(), "ancestor.jsonl", &base);
        let ours = write_events(tmp.path(), "ours.jsonl", &ours_events);
        let theirs = write_events(tmp.path(), "theirs.jsonl", &theirs_events);

        let result = merge_events_files(&ancestor, &ours, &theirs).unwrap();
        assert!(result.conflict);
        assert_eq!(result.conflicting_ids, vec!["01A"]);
    }

    #[test]
    fn test_empty_ancestor() {
        let tmp = TempDir::new().unwrap();
        let ancestor = write_events(tmp.path(), "ancestor.jsonl", &[]);
        let ours = write_events(tmp.path(), "ours.jsonl", &[make_event("01A", "a")]);
        let theirs = write_events(tmp.path(), "theirs.jsonl", &[make_event("01B", "b")]);

        let result = merge_events_files(&ancestor, &ours, &theirs).unwrap();
        assert!(!result.conflict);
        assert_eq!(result.total_events, 2);
        assert_eq!(result.duplicates_removed, 0);
    }
}
