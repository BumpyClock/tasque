use crate::domain::deps::normalize_dependency_edges;
use crate::domain::event_payload_codecs::{
    event_type_as_str, planning_state_from_str, relation_type_from_str, task_kind_from_str,
    task_status_as_str, task_status_from_str,
};
use crate::errors::TsqError;
use crate::types::{
    EventRecord, EventType, PlanningState, Priority, RelationType, State, Task, TaskKind,
    TaskStatus,
};
use serde_json::{Map, Value};
use std::collections::HashMap;

pub(crate) fn as_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(|value| value.to_string())
}

pub(crate) fn as_string_array(value: Option<&Value>) -> Option<Vec<String>> {
    let values = value?.as_array()?;
    if !values.iter().all(|entry| entry.is_string()) {
        return None;
    }
    Some(
        values
            .iter()
            .filter_map(|entry| entry.as_str())
            .map(|entry| entry.to_string())
            .collect(),
    )
}

pub(crate) fn as_priority(value: Option<&Value>) -> Option<Priority> {
    let raw = value?.as_i64()?;
    if !(0..=3).contains(&raw) {
        return None;
    }
    Some(raw as Priority)
}

pub(crate) fn as_bool(value: Option<&Value>) -> Option<bool> {
    value.and_then(Value::as_bool)
}

pub(crate) fn as_task_kind(value: Option<&Value>) -> Option<TaskKind> {
    task_kind_from_str(value?.as_str()?)
}

pub(crate) fn as_task_status(value: Option<&Value>) -> Option<TaskStatus> {
    task_status_from_str(value?.as_str()?)
}

pub(crate) fn as_planning_state(value: Option<&Value>) -> Option<PlanningState> {
    planning_state_from_str(value?.as_str()?)
}

pub(crate) fn as_relation_type(value: Option<&Value>) -> Option<RelationType> {
    relation_type_from_str(value?.as_str()?)
}

pub(crate) fn optional_task_kind_field(
    payload: &Map<String, Value>,
    field: &str,
    event: &EventRecord,
    event_name: &str,
) -> Result<Option<TaskKind>, TsqError> {
    optional_typed_field(payload, field, event, event_name, as_task_kind)
}

pub(crate) fn optional_priority_field(
    payload: &Map<String, Value>,
    field: &str,
    event: &EventRecord,
    event_name: &str,
) -> Result<Option<Priority>, TsqError> {
    optional_typed_field(payload, field, event, event_name, as_priority)
}

pub(crate) fn optional_task_status_field(
    payload: &Map<String, Value>,
    field: &str,
    event: &EventRecord,
    event_name: &str,
) -> Result<Option<TaskStatus>, TsqError> {
    optional_typed_field(payload, field, event, event_name, as_task_status)
}

pub(crate) fn optional_planning_state_field(
    payload: &Map<String, Value>,
    field: &str,
    event: &EventRecord,
    event_name: &str,
) -> Result<Option<PlanningState>, TsqError> {
    optional_typed_field(payload, field, event, event_name, as_planning_state)
}

pub(crate) fn optional_string_array_field(
    payload: &Map<String, Value>,
    field: &str,
    event: &EventRecord,
    event_name: &str,
) -> Result<Option<Vec<String>>, TsqError> {
    optional_typed_field(payload, field, event, event_name, as_string_array)
}

pub(crate) fn optional_task_ref_field(
    state: &State,
    payload: &Map<String, Value>,
    field: &str,
    event: &EventRecord,
    event_name: &str,
) -> Result<Option<String>, TsqError> {
    let Some(value) = present_non_null(payload, field) else {
        return Ok(None);
    };
    let Some(target) = value.as_str() else {
        return Err(invalid_event_field(event, event_name, field));
    };
    if target.is_empty() {
        return Err(invalid_event_field(event, event_name, field));
    }
    if target == event.task_id {
        return Err(TsqError::new(
            "INVALID_EVENT",
            format!("{} {} cannot reference self", event_name, field),
            1,
        )
        .with_details(serde_json::json!({
          "event_id": event_id_value(event),
          "task_id": &event.task_id,
          "field": field,
        })));
    }
    if !state.tasks.contains_key(target) {
        return Err(TsqError::new(
            "INVALID_EVENT",
            format!("{} {} references missing task", event_name, field),
            1,
        )
        .with_details(serde_json::json!({
          "event_id": event_id_value(event),
          "task_id": &event.task_id,
          "field": field,
          "target": target,
        })));
    }
    Ok(Some(target.to_string()))
}

fn optional_typed_field<T>(
    payload: &Map<String, Value>,
    field: &str,
    event: &EventRecord,
    event_name: &str,
    parse: fn(Option<&Value>) -> Option<T>,
) -> Result<Option<T>, TsqError> {
    let Some(value) = present_non_null(payload, field) else {
        return Ok(None);
    };
    parse(Some(value))
        .map(Some)
        .ok_or_else(|| invalid_event_field(event, event_name, field))
}

fn present_non_null<'a>(payload: &'a Map<String, Value>, field: &str) -> Option<&'a Value> {
    payload.get(field).filter(|value| !value.is_null())
}

fn invalid_event_field(event: &EventRecord, event_name: &str, field: &str) -> TsqError {
    TsqError::new(
        "INVALID_EVENT",
        format!("{} has invalid {}", event_name, field),
        1,
    )
    .with_details(serde_json::json!({
      "event_id": event_id_value(event),
      "task_id": &event.task_id,
      "field": field,
    }))
}

pub(crate) fn event_type_to_string(event_type: &EventType) -> &'static str {
    event_type_as_str(*event_type)
}

pub(crate) fn task_status_to_string(status: TaskStatus) -> &'static str {
    task_status_as_str(status)
}

pub(crate) fn event_id_value(event: &EventRecord) -> Option<String> {
    event
        .id
        .as_ref()
        .filter(|value| !value.is_empty())
        .cloned()
        .or_else(|| {
            event
                .event_id
                .as_ref()
                .filter(|value| !value.is_empty())
                .cloned()
        })
}

pub(crate) fn event_identifier(event: &EventRecord) -> Result<String, TsqError> {
    let id = event_id_value(event);
    if let Some(id) = id {
        return Ok(id);
    }
    Err(
        TsqError::new("INVALID_EVENT", "event requires id", 1).with_details(serde_json::json!({
          "type": event_type_to_string(&event.event_type),
          "task_id": &event.task_id,
        })),
    )
}

pub(crate) fn clone_state(state: &State) -> State {
    let deps = state
        .deps
        .iter()
        .map(|(id, edges)| {
            let normalized = normalize_dependency_edges(Some(edges));
            (id.clone(), normalized)
        })
        .collect::<HashMap<_, _>>();
    let links = state
        .links
        .iter()
        .map(|(id, rels)| {
            let mut rel_map = HashMap::new();
            for (rel_type, targets) in rels {
                rel_map.insert(*rel_type, targets.clone());
            }
            (id.clone(), rel_map)
        })
        .collect::<HashMap<_, _>>();
    State {
        tasks: state.tasks.clone(),
        deps,
        links,
        child_counters: state.child_counters.clone(),
        created_order: state.created_order.clone(),
        applied_events: state.applied_events,
    }
}

pub(crate) fn set_child_counter(state: &mut State, parent_id: &str, child_id: &str) {
    let prefix = format!("{}.", parent_id);
    if !child_id.starts_with(&prefix) {
        return;
    }
    let segment = &child_id[prefix.len()..];
    if segment.is_empty() || !segment.chars().all(|c| c.is_ascii_digit()) {
        return;
    }
    let Ok(counter) = segment.parse::<u32>() else {
        return;
    };
    let current = state.child_counters.get(parent_id).copied().unwrap_or(0);
    if counter > current {
        state.child_counters.insert(parent_id.to_string(), counter);
    }
}

pub(crate) fn set_task_closed_state(task: &Task, ts: &str) -> Task {
    let mut next = task.clone();
    next.status = TaskStatus::Closed;
    next.updated_at = ts.to_string();
    next.closed_at = Some(ts.to_string());
    next
}

pub(crate) fn upsert_directed_link(
    links: &mut HashMap<String, HashMap<RelationType, Vec<String>>>,
    src: &str,
    dst: &str,
    rel_type: RelationType,
) {
    let entry = links.entry(src.to_string()).or_default();
    let targets = entry.entry(rel_type).or_default();
    if !targets.iter().any(|candidate| candidate == dst) {
        targets.push(dst.to_string());
    }
}

pub(crate) fn remove_directed_link(
    links: &mut HashMap<String, HashMap<RelationType, Vec<String>>>,
    src: &str,
    dst: &str,
    rel_type: RelationType,
) {
    let Some(from) = links.get_mut(src) else {
        return;
    };
    let Some(current) = from.get_mut(&rel_type) else {
        return;
    };
    current.retain(|candidate| candidate != dst);
}

pub(crate) fn require_task<'a>(state: &'a State, task_id: &str) -> Result<&'a Task, TsqError> {
    state.tasks.get(task_id).ok_or_else(|| {
        TsqError::new("TASK_NOT_FOUND", "Task not found", 1).with_details(serde_json::json!({
          "task_id": task_id,
        }))
    })
}
