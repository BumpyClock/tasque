use crate::domain::projector::apply_events;
use crate::domain::state::create_empty_state;
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
/// 2. Build a stable union keyed by event ID
/// 3. Detect conflicts: same ID but different payload across files
/// 4. Deduplicate identical events
/// 5. Preserve source order: ancestor, then ours-only, then theirs-only
/// 6. Replay the merged events to validate causal ordering
/// 7. Write merged result to `ours` (git merge convention: result goes to %A)
pub fn merge_events_files(
    ancestor: &Path,
    ours: &Path,
    theirs: &Path,
) -> Result<MergeDriverOutcome, TsqError> {
    let ancestor_result = read_events_from_path(ancestor)?;
    let ours_result = read_events_from_path(ours)?;
    let theirs_result = read_events_from_path(theirs)?;

    // Map: event_id -> canonical_json
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut merged: Vec<(String, EventRecord)> = Vec::new();
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
                Some(existing_json) => {
                    if *existing_json != json && !conflicting_ids.contains(&id) {
                        conflicting_ids.push(id.clone());
                    }
                    // Same ID + same payload = duplicate, skip
                }
                None => {
                    seen.insert(id.clone(), json);
                    merged.push((id, record.clone()));
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

    let duplicates_removed = total_input.saturating_sub(merged.len());
    let total_events = merged.len();

    let replay_events: Vec<EventRecord> = merged.iter().map(|(_, record)| record.clone()).collect();
    apply_events(&create_empty_state(), &replay_events).map_err(|e| {
        TsqError::new(
            "MERGE_REPLAY_FAILED",
            format!("Merged event stream failed replay validation: {}", e),
            2,
        )
    })?;

    // Write merged result to ours path (git expects result at %A)
    write_events_to_path(ours, &merged)?;

    Ok(MergeDriverOutcome {
        total_events,
        duplicates_removed,
        conflict: false,
        conflicting_ids: Vec::new(),
    })
}

/// Write merged events to a file as JSONL.
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
