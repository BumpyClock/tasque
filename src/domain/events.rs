use crate::types::{EventRecord, EventType};
use serde_json::{Map, Value};
use ulid::Ulid;

pub fn make_event(
    actor: &str,
    ts: &str,
    event_type: EventType,
    task_id: &str,
    payload: Map<String, Value>,
) -> EventRecord {
    let id = Ulid::new().to_string();
    EventRecord {
        id: Some(id.clone()),
        event_id: Some(id),
        ts: ts.to_string(),
        actor: actor.to_string(),
        event_type,
        task_id: task_id.to_string(),
        payload,
    }
}
