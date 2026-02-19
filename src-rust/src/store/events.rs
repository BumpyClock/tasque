use crate::errors::TsqError;
use crate::store::paths::get_paths;
use crate::types::{EventRecord, EventType};
use serde_json::{Map, Value};
use std::fs::{OpenOptions, create_dir_all, read_to_string};
use std::io::Write;
use std::path::Path;

pub struct ReadEventsResult {
    pub events: Vec<EventRecord>,
    pub warning: Option<String>,
}

fn event_type_from_str(raw: &str) -> Option<EventType> {
    match raw {
        "task.created" => Some(EventType::TaskCreated),
        "task.updated" => Some(EventType::TaskUpdated),
        "task.status_set" => Some(EventType::TaskStatusSet),
        "task.claimed" => Some(EventType::TaskClaimed),
        "task.noted" => Some(EventType::TaskNoted),
        "task.spec_attached" => Some(EventType::TaskSpecAttached),
        "task.superseded" => Some(EventType::TaskSuperseded),
        "dep.added" => Some(EventType::DepAdded),
        "dep.removed" => Some(EventType::DepRemoved),
        "link.added" => Some(EventType::LinkAdded),
        "link.removed" => Some(EventType::LinkRemoved),
        _ => None,
    }
}

fn required_fields(event_type: &EventType) -> &'static [(&'static str, &'static str)] {
    match event_type {
        EventType::TaskCreated => &[("title", "string")],
        EventType::TaskUpdated => &[],
        EventType::TaskStatusSet => &[("status", "string")],
        EventType::TaskClaimed => &[],
        EventType::TaskNoted => &[("text", "string")],
        EventType::TaskSpecAttached => &[("spec_path", "string"), ("spec_fingerprint", "string")],
        EventType::TaskSuperseded => &[],
        EventType::DepAdded => &[("blocker", "string")],
        EventType::DepRemoved => &[("blocker", "string")],
        EventType::LinkAdded => &[("type", "string")],
        EventType::LinkRemoved => &[("type", "string")],
    }
}

fn validate_event_payload(
    event_type: &EventType,
    payload: &Map<String, Value>,
    line: usize,
) -> Result<(), TsqError> {
    for (field, expected) in required_fields(event_type) {
        let value = payload.get(*field);
        let type_mismatch = match *expected {
            "string" => value.and_then(Value::as_str).is_none(),
            _ => true,
        };
        if value.is_none() || type_mismatch {
            return Err(TsqError::new(
                "EVENTS_CORRUPT",
                format!(
                    "Invalid event at line {}: {} payload missing required field \"{}\" (expected {})",
                    line,
                    event_type_to_string(event_type),
                    field,
                    expected
                ),
                2,
            ));
        }
    }

    if matches!(event_type, EventType::DepAdded | EventType::DepRemoved)
        && let Some(dep_type_value) = payload.get("dep_type")
    {
        let dep_type_str = dep_type_value.as_str().unwrap_or("");
        if dep_type_str != "blocks" && dep_type_str != "starts_after" {
            return Err(TsqError::new(
                "EVENTS_CORRUPT",
                format!(
                    "Invalid event at line {}: {} payload field \"dep_type\" must be blocks|starts_after",
                    line,
                    event_type_to_string(event_type)
                ),
                2,
            ));
        }
    }

    Ok(())
}

fn event_type_to_string(event_type: &EventType) -> &'static str {
    match event_type {
        EventType::TaskCreated => "task.created",
        EventType::TaskUpdated => "task.updated",
        EventType::TaskStatusSet => "task.status_set",
        EventType::TaskClaimed => "task.claimed",
        EventType::TaskNoted => "task.noted",
        EventType::TaskSpecAttached => "task.spec_attached",
        EventType::TaskSuperseded => "task.superseded",
        EventType::DepAdded => "dep.added",
        EventType::DepRemoved => "dep.removed",
        EventType::LinkAdded => "link.added",
        EventType::LinkRemoved => "link.removed",
    }
}

fn parse_event_record(value: &Value, line: usize) -> Result<EventRecord, TsqError> {
    let obj = value.as_object().ok_or_else(|| {
        TsqError::new(
            "EVENTS_CORRUPT",
            format!("Invalid event at line {}: expected record", line),
            2,
        )
    })?;

    let id = obj
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let event_id = obj
        .get("event_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    if id.is_none() && event_id.is_none() {
        return Err(TsqError::new(
            "EVENTS_CORRUPT",
            format!(
                "Invalid event at line {}: missing required field \"id\" (or legacy \"event_id\")",
                line
            ),
            2,
        ));
    }

    let ts = obj
        .get("ts")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    if ts.is_none() {
        return Err(TsqError::new(
            "EVENTS_CORRUPT",
            format!("Invalid event at line {}: ts must be a string", line),
            2,
        ));
    }

    let actor = obj
        .get("actor")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    if actor.is_none() {
        return Err(TsqError::new(
            "EVENTS_CORRUPT",
            format!("Invalid event at line {}: actor must be a string", line),
            2,
        ));
    }

    let event_type_raw = obj
        .get("type")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let event_type_raw = match event_type_raw {
        Some(value) => value,
        None => {
            return Err(TsqError::new(
                "EVENTS_CORRUPT",
                format!("Invalid event at line {}: type must be a string", line),
                2,
            ));
        }
    };

    let event_type = event_type_from_str(event_type_raw).ok_or_else(|| {
        TsqError::new(
            "EVENTS_CORRUPT",
            format!(
                "Invalid event at line {}: unknown event type \"{}\"",
                line, event_type_raw
            ),
            2,
        )
    })?;

    let task_id = obj
        .get("task_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    if task_id.is_none() {
        return Err(TsqError::new(
            "EVENTS_CORRUPT",
            format!("Invalid event at line {}: task_id must be a string", line),
            2,
        ));
    }

    let payload_value = obj.get("payload").ok_or_else(|| {
        TsqError::new(
            "EVENTS_CORRUPT",
            format!("Invalid event at line {}: payload must be an object", line),
            2,
        )
    })?;
    let payload = payload_value.as_object().ok_or_else(|| {
        TsqError::new(
            "EVENTS_CORRUPT",
            format!("Invalid event at line {}: expected record", line),
            2,
        )
    })?;

    validate_event_payload(&event_type, payload, line)?;

    let normalized_id = id.or(event_id).unwrap();
    let mut payload_map = Map::new();
    for (key, value) in payload.iter() {
        payload_map.insert(key.clone(), value.clone());
    }

    Ok(EventRecord {
        id: Some(normalized_id.to_string()),
        event_id: Some(normalized_id.to_string()),
        ts: ts.unwrap().to_string(),
        actor: actor.unwrap().to_string(),
        event_type,
        task_id: task_id.unwrap().to_string(),
        payload: payload_map,
    })
}

pub fn append_events(repo_root: impl AsRef<Path>, events: &[EventRecord]) -> Result<(), TsqError> {
    if events.is_empty() {
        return Ok(());
    }

    let paths = get_paths(repo_root);
    create_dir_all(&paths.tasque_dir).map_err(|error| {
        TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
            .with_details(io_error_value(&error))
    })?;

    let payload = events
        .iter()
        .map(|event| {
            serde_json::to_string(event).map_err(|error| {
                TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                    .with_details(any_error_value(&error))
            })
        })
        .collect::<Result<Vec<String>, TsqError>>()?
        .join("\n")
        + "\n";

    let mut handle = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&paths.events_file)
        .map_err(|error| {
            TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                .with_details(io_error_value(&error))
        })?;
    if let Err(error) = handle.write_all(payload.as_bytes()) {
        return Err(
            TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                .with_details(io_error_value(&error)),
        );
    }
    if let Err(error) = handle.sync_all() {
        return Err(
            TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                .with_details(io_error_value(&error)),
        );
    }

    Ok(())
}

pub fn read_events(repo_root: impl AsRef<Path>) -> Result<ReadEventsResult, TsqError> {
    let paths = get_paths(repo_root);

    let raw = match read_to_string(&paths.events_file) {
        Ok(raw) => raw,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return Ok(ReadEventsResult {
                    events: Vec::new(),
                    warning: None,
                });
            }
            return Err(
                TsqError::new("EVENT_READ_FAILED", "Failed reading events", 2)
                    .with_details(io_error_value(&error)),
            );
        }
    };

    let mut lines: Vec<&str> = raw.split('\n').collect();
    if matches!(lines.last(), Some(value) if value.is_empty()) {
        lines.pop();
    }

    let mut events = Vec::new();
    let mut warning = None;

    for (index, line) in lines.iter().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<Value>(line) {
            Ok(parsed) => match parse_event_record(&parsed, index + 1) {
                Ok(record) => events.push(record),
                Err(error) => return Err(error),
            },
            Err(_error) => {
                if index == lines.len() - 1 {
                    warning = Some(format!(
                        "Ignored malformed trailing JSONL line in {}",
                        paths.events_file.display()
                    ));
                    break;
                }
                return Err(TsqError::new(
                    "EVENTS_CORRUPT",
                    format!("Malformed events JSONL at line {}", index + 1),
                    2,
                ));
            }
        }
    }

    Ok(ReadEventsResult { events, warning })
}

fn io_error_value(error: &std::io::Error) -> Value {
    serde_json::json!({"kind": format!("{:?}", error.kind()), "message": error.to_string()})
}

fn any_error_value(error: &impl std::fmt::Display) -> Value {
    serde_json::json!({"message": error.to_string()})
}
