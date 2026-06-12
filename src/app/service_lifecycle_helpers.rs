use crate::domain::events::make_event;
use crate::types::{EventRecord, EventType, RelationType, TaskStatus};
use serde_json::{Map, Value};

pub fn payload_map(value: Value) -> Map<String, Value> {
    match value.as_object() {
        Some(map) => map.clone(),
        None => Map::new(),
    }
}

pub fn duplicate_close_events(
    actor: &str,
    ts: &str,
    source: &str,
    canonical: &str,
    reason: Option<&str>,
    has_existing_link: bool,
) -> Vec<EventRecord> {
    let mut events = Vec::new();
    if !has_existing_link {
        events.push(make_event(
            actor,
            ts,
            EventType::LinkAdded,
            source,
            payload_map(serde_json::json!({"type": RelationType::Duplicates, "target": canonical})),
        ));
    }
    events.push(make_event(
        actor,
        ts,
        EventType::TaskUpdated,
        source,
        payload_map(serde_json::json!({"duplicate_of": canonical})),
    ));
    let mut payload =
        payload_map(serde_json::json!({"status": TaskStatus::Closed, "closed_at": ts}));
    if let Some(reason) = reason {
        payload.insert("reason".to_string(), Value::String(reason.to_string()));
    }
    events.push(make_event(
        actor,
        ts,
        EventType::TaskStatusSet,
        source,
        payload,
    ));
    events
}

pub fn status_to_string(status: TaskStatus) -> String {
    crate::domain::event_payload_codecs::task_status_as_str(status)
}
