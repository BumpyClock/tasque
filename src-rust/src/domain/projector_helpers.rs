use crate::domain::deps::normalize_dependency_edges;
use crate::errors::TsqError;
use crate::types::{
    EventRecord, EventType, PlanningState, Priority, RelationType, State, Task, TaskKind,
    TaskStatus,
};
use serde_json::Value;
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
    if raw < 0 || raw > 3 {
        return None;
    }
    Some(raw as Priority)
}

pub(crate) fn as_bool(value: Option<&Value>) -> Option<bool> {
    value.and_then(Value::as_bool)
}

pub(crate) fn as_task_kind(value: Option<&Value>) -> Option<TaskKind> {
    match value?.as_str()? {
        "task" => Some(TaskKind::Task),
        "feature" => Some(TaskKind::Feature),
        "epic" => Some(TaskKind::Epic),
        _ => None,
    }
}

pub(crate) fn as_task_status(value: Option<&Value>) -> Option<TaskStatus> {
    match value?.as_str()? {
        "open" => Some(TaskStatus::Open),
        "in_progress" => Some(TaskStatus::InProgress),
        "blocked" => Some(TaskStatus::Blocked),
        "closed" => Some(TaskStatus::Closed),
        "canceled" => Some(TaskStatus::Canceled),
        "deferred" => Some(TaskStatus::Deferred),
        _ => None,
    }
}

pub(crate) fn as_planning_state(value: Option<&Value>) -> Option<PlanningState> {
    match value?.as_str()? {
        "needs_planning" => Some(PlanningState::NeedsPlanning),
        "planned" => Some(PlanningState::Planned),
        _ => None,
    }
}

pub(crate) fn as_relation_type(value: Option<&Value>) -> Option<RelationType> {
    match value?.as_str()? {
        "relates_to" => Some(RelationType::RelatesTo),
        "replies_to" => Some(RelationType::RepliesTo),
        "duplicates" => Some(RelationType::Duplicates),
        "supersedes" => Some(RelationType::Supersedes),
        _ => None,
    }
}

pub(crate) fn event_type_to_string(event_type: &EventType) -> &'static str {
    #[allow(unreachable_patterns)]
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
        _ => "unknown",
    }
}

pub(crate) fn task_status_to_string(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Open => "open",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Closed => "closed",
        TaskStatus::Canceled => "canceled",
        TaskStatus::Deferred => "deferred",
    }
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
    let entry = links.entry(src.to_string()).or_insert_with(HashMap::new);
    let targets = entry.entry(rel_type).or_insert_with(Vec::new);
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
