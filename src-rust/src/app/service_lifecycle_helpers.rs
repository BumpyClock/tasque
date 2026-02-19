use crate::types::TaskStatus;
use serde_json::{Map, Value};

pub fn payload_map(value: Value) -> Map<String, Value> {
    match value.as_object() {
        Some(map) => map.clone(),
        None => Map::new(),
    }
}

pub fn status_to_string(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Open => "open",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Closed => "closed",
        TaskStatus::Canceled => "canceled",
        TaskStatus::Deferred => "deferred",
    }
}
