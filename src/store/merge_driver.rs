use crate::domain::projector::apply_events;
use crate::domain::state::create_empty_state;
use crate::errors::TsqError;
use crate::store::events::read_events_from_path;
use crate::types::{EventRecord, MergeDriverOutcome};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::Path;

/// Extract the canonical event ID from a reader-normalized EventRecord.
fn event_id(record: &EventRecord) -> Result<&str, TsqError> {
    record
        .id
        .as_deref()
        .or(record.event_id.as_deref())
        .ok_or_else(|| {
            TsqError::new(
                "EVENTS_CORRUPT",
                "Event missing id field during merge after read validation",
                2,
            )
        })
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

fn order_key(id: &str, records: &HashMap<String, EventRecord>) -> Reverse<(String, String)> {
    let record = records.get(id).expect("id present in map");
    Reverse((record.ts.clone(), id.to_string()))
}

/// Merge three versions of an events.jsonl file (ancestor, ours, theirs).
///
/// Algorithm:
/// 1. Read all three files
/// 2. Build a union keyed by event ID (prefers `id`, falls back to `event_id`)
/// 3. Detect conflicts: same ID but different payload across files
/// 4. Deduplicate identical events
/// 5. Produce a deterministic order via a topological sort that:
///    - preserves each source's internal causal order (event[i] before event[i+1])
///    - breaks ties by smallest event ID (min-heap)
///      This makes the merged output byte-identical for `A<-B` and `B<-A` when
///      both sides add independent events, while still keeping causal chains
///      (e.g. create-before-update) intact even when IDs sort backwards.
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

    // id -> (record, canonical_json). First occurrence wins for the payload.
    let mut id_to_record: HashMap<String, EventRecord> = HashMap::new();
    let mut id_to_json: HashMap<String, String> = HashMap::new();
    let mut conflicting_ids: HashSet<String> = HashSet::new();
    let mut total_input = 0usize;

    // Per-source ordered id sequences, used to derive causal edges.
    let mut source_orders: Vec<Vec<String>> = Vec::new();

    for events in [
        ancestor_result.events,
        ours_result.events,
        theirs_result.events,
    ] {
        let mut order: Vec<String> = Vec::new();
        for record in events {
            total_input += 1;
            let id = event_id(&record)?.to_string();
            let json = canonical_json(&record)?;
            match id_to_json.get(&id) {
                Some(existing_json) => {
                    if *existing_json != json {
                        conflicting_ids.insert(id.clone());
                    }
                    // Same ID + same payload = duplicate, nothing to store.
                }
                None => {
                    id_to_record.insert(id.clone(), record);
                    id_to_json.insert(id.clone(), json);
                }
            }
            order.push(id);
        }
        source_orders.push(order);
    }

    let unique_count = id_to_record.len();
    let duplicates_removed = total_input.saturating_sub(unique_count);

    if !conflicting_ids.is_empty() {
        let mut ids: Vec<String> = conflicting_ids.into_iter().collect();
        ids.sort();
        return Ok(MergeDriverOutcome {
            total_events: unique_count,
            duplicates_removed,
            conflict: true,
            conflicting_ids: ids,
        });
    }

    // Derive causal edges from each source's order. Dedupe edges across sources
    // so the incoming-edge count stays accurate.
    let mut edges: HashSet<(String, String)> = HashSet::new();
    for order in &source_orders {
        for window in order.windows(2) {
            if window[0] != window[1] {
                edges.insert((window[0].clone(), window[1].clone()));
            }
        }
    }

    let mut incoming: HashMap<String, usize> = HashMap::new();
    let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();
    for id in id_to_record.keys() {
        incoming.entry(id.clone()).or_insert(0);
        outgoing.entry(id.clone()).or_default();
    }
    for (from, to) in &edges {
        outgoing.entry(from.clone()).or_default().push(to.clone());
        *incoming.entry(to.clone()).or_default() += 1;
    }

    // Kahn's algorithm with a min-heap keyed by (timestamp, event ID). The
    // timestamp primary key makes last-write-wins projection explicit for
    // independent updates; the event ID secondary key keeps output stable when
    // timestamps collide.
    let mut heap: BinaryHeap<Reverse<(String, String)>> = incoming
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| order_key(id, &id_to_record))
        .collect();
    let mut sorted_ids: Vec<String> = Vec::with_capacity(unique_count);
    while let Some(Reverse((_ts, id))) = heap.pop() {
        sorted_ids.push(id.clone());
        if let Some(nexts) = outgoing.get(&id) {
            for next in nexts {
                let count = incoming.entry(next.clone()).or_insert(0);
                *count -= 1;
                if *count == 0 {
                    heap.push(order_key(next, &id_to_record));
                }
            }
        }
    }

    // Cycle fallback: if constraints formed a cycle (same events recorded in
    // conflicting orders across sources), append the remaining IDs in the same
    // deterministic (timestamp, event ID) order rather than failing silently.
    if sorted_ids.len() < unique_count {
        let emitted: HashSet<String> = sorted_ids.iter().cloned().collect();
        let mut remaining: Vec<String> = id_to_record
            .keys()
            .filter(|id| !emitted.contains(*id))
            .cloned()
            .collect();
        remaining.sort_by_key(|id| {
            let record = id_to_record.get(id).expect("id present in map");
            (record.ts.clone(), id.clone())
        });
        sorted_ids.extend(remaining);
    }

    let merged: Vec<EventRecord> = sorted_ids
        .iter()
        .map(|id| id_to_record.get(id).expect("id present in map").clone())
        .collect();

    apply_events(&create_empty_state(), &merged).map_err(|e| {
        TsqError::new(
            "MERGE_REPLAY_FAILED",
            format!("Merged event stream failed replay validation: {}", e),
            2,
        )
    })?;

    let total_events = merged.len();
    write_events_to_path(ours, &merged)?;

    Ok(MergeDriverOutcome {
        total_events,
        duplicates_removed,
        conflict: false,
        conflicting_ids: Vec::new(),
    })
}

/// Write merged events to a file as JSONL.
fn write_events_to_path(path: &Path, events: &[EventRecord]) -> Result<(), TsqError> {
    let mut file = fs::File::create(path).map_err(|e| {
        TsqError::new(
            "MERGE_WRITE_FAILED",
            format!("Failed writing merged events to {}: {}", path.display(), e),
            2,
        )
    })?;

    for record in events {
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
