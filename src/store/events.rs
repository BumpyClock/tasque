use crate::domain::event_payload_codecs::{
    dependency_type_from_str, event_type_as_str, event_type_from_str, planning_state_from_str,
    relation_type_from_str, task_kind_from_str, task_status_from_str,
};
use crate::errors::TsqError;
use crate::store::paths::get_paths;
use crate::types::{EventLogMetadata, EventRecord, EventType};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs::{OpenOptions, create_dir_all, read, read_to_string};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

pub struct ReadEventsResult {
    pub events: Vec<EventRecord>,
    pub warning: Option<String>,
    pub metadata: EventLogMetadata,
}

fn required_fields(event_type: &EventType) -> &'static [(&'static str, &'static str)] {
    match event_type {
        EventType::TaskCreated => &[("title", "string")],
        EventType::TaskUpdated => &[],
        EventType::TaskStatusSet => &[("status", "string")],
        EventType::TaskClaimed => &[],
        EventType::TaskNoted => &[("text", "string")],
        EventType::TaskSpecAttached => &[("spec_path", "string"), ("spec_fingerprint", "string")],
        EventType::TaskSuperseded => &[("with", "string")],
        EventType::DepAdded => &[("blocker", "string")],
        EventType::DepRemoved => &[("blocker", "string")],
        EventType::LinkAdded => &[("type", "string"), ("target", "string")],
        EventType::LinkRemoved => &[("type", "string"), ("target", "string")],
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
            "string" => value
                .and_then(Value::as_str)
                .filter(|raw| !raw.is_empty())
                .is_none(),
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
        validate_enum_field(
            event_type,
            "dep_type",
            dep_type_value,
            line,
            dependency_type_from_str,
        )?;
    }
    if matches!(event_type, EventType::TaskCreated) {
        validate_optional_enum_field(event_type, payload, "kind", line, task_kind_from_str)?;
        validate_optional_enum_field(event_type, payload, "status", line, task_status_from_str)?;
        validate_optional_enum_field(
            event_type,
            payload,
            "planning_state",
            line,
            planning_state_from_str,
        )?;
    }
    if matches!(event_type, EventType::TaskUpdated) {
        validate_optional_enum_field(event_type, payload, "kind", line, task_kind_from_str)?;
        validate_optional_enum_field(event_type, payload, "status", line, task_status_from_str)?;
        validate_optional_enum_field(
            event_type,
            payload,
            "planning_state",
            line,
            planning_state_from_str,
        )?;
    }
    if matches!(event_type, EventType::TaskStatusSet)
        && let Some(status_value) = payload.get("status")
    {
        validate_enum_field(
            event_type,
            "status",
            status_value,
            line,
            task_status_from_str,
        )?;
    }
    if matches!(event_type, EventType::LinkAdded | EventType::LinkRemoved)
        && let Some(type_value) = payload.get("type")
    {
        validate_enum_field(event_type, "type", type_value, line, relation_type_from_str)?;
    }
    validate_optional_priority(event_type, payload, line)?;
    validate_optional_labels(event_type, payload, line)?;
    for field in [
        "clear_description",
        "clear_external_ref",
        "clear_discovered_from",
    ] {
        validate_optional_bool(event_type, payload, field, line)?;
    }
    for field in [
        "parent_id",
        "superseded_by",
        "duplicate_of",
        "replies_to",
        "discovered_from",
        "with",
        "blocker",
        "target",
    ] {
        validate_optional_nonempty_string(event_type, payload, field, line)?;
    }

    Ok(())
}

fn validate_optional_priority(
    event_type: &EventType,
    payload: &Map<String, Value>,
    line: usize,
) -> Result<(), TsqError> {
    if let Some(value) = payload.get("priority") {
        let Some(priority) = value.as_u64() else {
            return Err(invalid_event_payload_field(
                event_type,
                "priority",
                line,
                "must be an integer 0..=3",
            ));
        };
        if priority > 3 {
            return Err(invalid_event_payload_field(
                event_type,
                "priority",
                line,
                "must be an integer 0..=3",
            ));
        }
    }
    Ok(())
}

fn validate_optional_labels(
    event_type: &EventType,
    payload: &Map<String, Value>,
    line: usize,
) -> Result<(), TsqError> {
    if let Some(value) = payload.get("labels") {
        let Some(labels) = value.as_array() else {
            return Err(invalid_event_payload_field(
                event_type,
                "labels",
                line,
                "must be an array of strings",
            ));
        };
        if labels.iter().any(|label| label.as_str().is_none()) {
            return Err(invalid_event_payload_field(
                event_type,
                "labels",
                line,
                "must be an array of strings",
            ));
        }
    }
    Ok(())
}

fn validate_optional_bool(
    event_type: &EventType,
    payload: &Map<String, Value>,
    field: &'static str,
    line: usize,
) -> Result<(), TsqError> {
    if let Some(value) = payload.get(field)
        && value.as_bool().is_none()
    {
        return Err(invalid_event_payload_field(
            event_type,
            field,
            line,
            "must be a boolean",
        ));
    }
    Ok(())
}

fn validate_optional_nonempty_string(
    event_type: &EventType,
    payload: &Map<String, Value>,
    field: &'static str,
    line: usize,
) -> Result<(), TsqError> {
    if let Some(value) = payload.get(field) {
        if value.is_null() {
            return Ok(());
        }
        if value.as_str().filter(|raw| !raw.is_empty()).is_none() {
            return Err(invalid_event_payload_field(
                event_type,
                field,
                line,
                "must be a nonempty string",
            ));
        }
    }
    Ok(())
}

fn validate_optional_enum_field<T>(
    event_type: &EventType,
    payload: &Map<String, Value>,
    field: &'static str,
    line: usize,
    parse: fn(&str) -> Option<T>,
) -> Result<(), TsqError> {
    if let Some(value) = payload.get(field) {
        if value.is_null() {
            return Ok(());
        }
        validate_enum_field(event_type, field, value, line, parse)?;
    }
    Ok(())
}

fn validate_enum_field<T>(
    event_type: &EventType,
    field: &'static str,
    value: &Value,
    line: usize,
    parse: fn(&str) -> Option<T>,
) -> Result<(), TsqError> {
    let raw = value.as_str().unwrap_or("");
    if parse(raw).is_none() {
        return Err(invalid_event_payload_field(
            event_type,
            field,
            line,
            "invalid enum value",
        ));
    }
    Ok(())
}

fn invalid_event_payload_field(
    event_type: &EventType,
    field: &str,
    line: usize,
    reason: &str,
) -> TsqError {
    TsqError::new(
        "EVENTS_CORRUPT",
        format!(
            "Invalid event at line {}: {} payload field \"{}\" {}",
            line,
            event_type_to_string(event_type),
            field,
            reason
        ),
        2,
    )
}

fn event_type_to_string(event_type: &EventType) -> &'static str {
    event_type_as_str(*event_type)
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

fn validate_event_for_append(event: &EventRecord) -> Result<(), TsqError> {
    let value = serde_json::to_value(event).map_err(|error| {
        TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
            .with_details(any_error_value(&error))
    })?;
    parse_event_record(&value, 0).map(|_| ()).map_err(|error| {
        TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2).with_details(
            serde_json::json!({
                "validation_code": error.code,
                "message": error.message,
            }),
        )
    })
}

fn prepare_event_file_for_append(
    handle: &mut std::fs::File,
    path: &Path,
) -> Result<bool, TsqError> {
    let bytes = read(path).map_err(|error| {
        TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
            .with_details(io_error_value(&error))
    })?;
    if bytes.is_empty() {
        handle.seek(SeekFrom::End(0)).map_err(|error| {
            TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                .with_details(io_error_value(&error))
        })?;
        return Ok(false);
    }

    let raw = std::str::from_utf8(&bytes).map_err(|error| {
        TsqError::new("EVENTS_CORRUPT", "Events file is not valid UTF-8", 2)
            .with_details(any_error_value(&error))
    })?;
    let mut nonempty_lines: Vec<(usize, &str, usize)> = Vec::new();
    let mut offset = 0;
    for (line_index, raw_line) in raw.split_inclusive('\n').enumerate() {
        let start = offset;
        offset += raw_line.len();
        let line = raw_line
            .strip_suffix('\n')
            .unwrap_or(raw_line)
            .trim_end_matches('\r');
        if !line.trim().is_empty() {
            nonempty_lines.push((start, line, line_index + 1));
        }
    }

    let Some(final_index) = nonempty_lines.len().checked_sub(1) else {
        handle.seek(SeekFrom::End(0)).map_err(|error| {
            TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                .with_details(io_error_value(&error))
        })?;
        return Ok(false);
    };

    for (index, (_start, line, line_number)) in nonempty_lines.iter().enumerate() {
        match serde_json::from_str::<Value>(line) {
            Ok(parsed) => {
                parse_event_record(&parsed, *line_number)?;
            }
            Err(_) if index == final_index => {}
            Err(_) => {
                return Err(TsqError::new(
                    "EVENTS_CORRUPT",
                    format!("Malformed events JSONL at line {}", line_number),
                    2,
                ));
            }
        }
    }

    let (last_start, final_line, _line_number) = nonempty_lines[final_index];
    match serde_json::from_str::<Value>(final_line) {
        Ok(_) => {
            handle.seek(SeekFrom::End(0)).map_err(|error| {
                TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                    .with_details(io_error_value(&error))
            })?;
            Ok(!bytes.ends_with(b"\n"))
        }
        Err(_) => {
            handle.set_len(last_start as u64).map_err(|error| {
                TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                    .with_details(io_error_value(&error))
            })?;
            handle.seek(SeekFrom::End(0)).map_err(|error| {
                TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                    .with_details(io_error_value(&error))
            })?;
            Ok(false)
        }
    }
}

pub fn append_events(repo_root: impl AsRef<Path>, events: &[EventRecord]) -> Result<(), TsqError> {
    if events.is_empty() {
        return Ok(());
    }

    for event in events {
        validate_event_for_append(event)?;
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
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&paths.events_file)
        .map_err(|error| {
            TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                .with_details(io_error_value(&error))
        })?;
    let needs_separator = prepare_event_file_for_append(&mut handle, &paths.events_file)?;
    if needs_separator {
        handle.write_all(b"\n").map_err(|error| {
            TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                .with_details(io_error_value(&error))
        })?;
    }
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

pub fn read_events_from_path(path: &Path) -> Result<ReadEventsResult, TsqError> {
    let raw = match read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return Ok(ReadEventsResult {
                    events: Vec::new(),
                    warning: None,
                    metadata: EventLogMetadata {
                        event_count: 0,
                        byte_len: 0,
                        sha256: sha256_hex(&[]),
                    },
                });
            }
            return Err(
                TsqError::new("EVENT_READ_FAILED", "Failed reading events", 2)
                    .with_details(io_error_value(&error)),
            );
        }
    };
    let byte_len = raw.len() as u64;
    let sha256 = sha256_hex(raw.as_bytes());
    let (events, warning) = parse_events_raw(&raw, path, 0)?;

    Ok(ReadEventsResult {
        metadata: EventLogMetadata {
            event_count: events.len(),
            byte_len,
            sha256,
        },
        events,
        warning,
    })
}

pub fn read_event_log_metadata(
    repo_root: impl AsRef<Path>,
    event_count: usize,
) -> Result<EventLogMetadata, TsqError> {
    let paths = get_paths(repo_root);
    let raw = match read(&paths.events_file) {
        Ok(raw) => raw,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                Vec::new()
            } else {
                return Err(
                    TsqError::new("EVENT_READ_FAILED", "Failed reading events", 2)
                        .with_details(io_error_value(&error)),
                );
            }
        }
    };
    Ok(EventLogMetadata {
        event_count,
        byte_len: raw.len() as u64,
        sha256: sha256_hex(&raw),
    })
}

pub fn read_events_tail_from_path(
    path: &Path,
    prefix: &EventLogMetadata,
) -> Result<Option<ReadEventsResult>, TsqError> {
    let raw = match read(path) {
        Ok(raw) => raw,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(
                TsqError::new("EVENT_READ_FAILED", "Failed reading events", 2)
                    .with_details(io_error_value(&error)),
            );
        }
    };
    if raw.len() < prefix.byte_len as usize {
        return Ok(None);
    }

    let prefix_len = prefix.byte_len as usize;
    if sha256_hex(&raw[..prefix_len]) != prefix.sha256 {
        return Ok(None);
    }

    let tail = std::str::from_utf8(&raw[prefix_len..]).map_err(|error| {
        TsqError::new("EVENTS_CORRUPT", "Events file is not valid UTF-8", 2)
            .with_details(any_error_value(&error))
    })?;
    let (events, warning) = parse_events_raw(tail, path, prefix.event_count)?;
    let metadata = EventLogMetadata {
        event_count: prefix.event_count + events.len(),
        byte_len: raw.len() as u64,
        sha256: sha256_hex(&raw),
    };

    Ok(Some(ReadEventsResult {
        events,
        warning,
        metadata,
    }))
}

fn parse_events_raw(
    raw: &str,
    path: &Path,
    line_offset: usize,
) -> Result<(Vec<EventRecord>, Option<String>), TsqError> {
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
            Ok(parsed) => match parse_event_record(&parsed, line_offset + index + 1) {
                Ok(record) => events.push(record),
                Err(error) => return Err(error),
            },
            Err(_error) => {
                if index == lines.len() - 1 {
                    warning = Some(format!(
                        "Ignored malformed trailing JSONL line in {}",
                        path.display()
                    ));
                    break;
                }
                return Err(TsqError::new(
                    "EVENTS_CORRUPT",
                    format!("Malformed events JSONL at line {}", line_offset + index + 1),
                    2,
                ));
            }
        }
    }

    Ok((events, warning))
}

pub fn read_events(repo_root: impl AsRef<Path>) -> Result<ReadEventsResult, TsqError> {
    let paths = get_paths(repo_root);
    read_events_from_path(&paths.events_file)
}

fn io_error_value(error: &std::io::Error) -> Value {
    serde_json::json!({"kind": format!("{:?}", error.kind()), "message": error.to_string()})
}

fn any_error_value(error: &impl std::fmt::Display) -> Value {
    serde_json::json!({"message": error.to_string()})
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
