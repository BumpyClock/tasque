use super::projector_helpers::{
    as_bool, as_planning_state, as_priority, as_string, as_string_array, as_task_kind,
    as_task_status, event_id_value, event_identifier, require_task, set_child_counter,
    set_task_closed_state, task_status_to_string,
};
use crate::errors::TsqError;
use crate::types::{EventRecord, PlanningState, Task, TaskNote, TaskStatus};

pub(crate) fn apply_task_created(
    state: &mut crate::types::State,
    event: &EventRecord,
) -> Result<(), TsqError> {
    if state.tasks.contains_key(&event.task_id) {
        return Err(
            TsqError::new("TASK_EXISTS", "Task already exists", 1).with_details(
                serde_json::json!({
                  "task_id": &event.task_id,
                }),
            ),
        );
    }
    let payload = &event.payload;
    let title = as_string(payload.get("title"));
    if title
        .as_deref()
        .map(|value| value.is_empty())
        .unwrap_or(true)
    {
        return Err(
            TsqError::new("INVALID_EVENT", "task.created requires a title", 1).with_details(
                serde_json::json!({
                  "event_id": event_id_value(event),
                }),
            ),
        );
    }

    let kind = as_task_kind(payload.get("kind")).unwrap_or(crate::types::TaskKind::Task);
    let priority = as_priority(payload.get("priority")).unwrap_or(1);
    let status = as_task_status(payload.get("status")).unwrap_or(TaskStatus::Open);
    let labels = as_string_array(payload.get("labels")).unwrap_or_default();
    let parent_id = as_string(payload.get("parent_id"));
    let planning_state =
        as_planning_state(payload.get("planning_state")).unwrap_or(PlanningState::NeedsPlanning);
    let discovered_from = as_string(payload.get("discovered_from"));
    if let Some(ref discovered_from) = discovered_from {
        if discovered_from == &event.task_id {
            return Err(TsqError::new(
                "INVALID_EVENT",
                "task.created discovered_from cannot reference self",
                1,
            )
            .with_details(serde_json::json!({
              "event_id": event_id_value(event),
            })));
        }
        require_task(state, discovered_from)?;
    }

    let task = Task {
        id: event.task_id.clone(),
        kind,
        title: title.unwrap(),
        description: as_string(payload.get("description")),
        notes: Vec::new(),
        spec_path: None,
        spec_fingerprint: None,
        spec_attached_at: None,
        spec_attached_by: None,
        status,
        priority,
        assignee: as_string(payload.get("assignee")),
        external_ref: as_string(payload.get("external_ref")),
        discovered_from,
        parent_id: parent_id.clone(),
        superseded_by: as_string(payload.get("superseded_by")),
        duplicate_of: as_string(payload.get("duplicate_of")),
        planning_state: Some(planning_state),
        replies_to: as_string(payload.get("replies_to")),
        labels,
        created_at: event.ts.clone(),
        updated_at: event.ts.clone(),
        closed_at: if status == TaskStatus::Closed {
            Some(event.ts.clone())
        } else {
            None
        },
    };

    state.tasks.insert(event.task_id.clone(), task);
    state.created_order.push(event.task_id.clone());
    if let Some(parent_id) = parent_id {
        set_child_counter(state, &parent_id, &event.task_id);
    }

    Ok(())
}

pub(crate) fn apply_task_updated(
    state: &mut crate::types::State,
    event: &EventRecord,
) -> Result<(), TsqError> {
    let current = require_task(state, &event.task_id)?.clone();
    let payload = &event.payload;
    let mut next = current.clone();
    next.updated_at = event.ts.clone();

    let title = as_string(payload.get("title"));
    if let Some(title) = title {
        if title.is_empty() {
            return Err(
                TsqError::new("INVALID_EVENT", "task.updated title must not be empty", 1)
                    .with_details(serde_json::json!({
                      "event_id": event_id_value(event),
                    })),
            );
        }
        next.title = title;
    }

    if let Some(kind) = as_task_kind(payload.get("kind")) {
        next.kind = kind;
    }

    if let Some(priority) = as_priority(payload.get("priority")) {
        next.priority = priority;
    }

    let assignee = as_string(payload.get("assignee"));
    if let Some(assignee) = assignee {
        next.assignee = Some(assignee);
    }

    let labels = as_string_array(payload.get("labels"));
    if let Some(labels) = labels {
        next.labels = labels;
    }

    let duplicate_of = as_string(payload.get("duplicate_of"));
    if let Some(duplicate_of) = duplicate_of {
        if duplicate_of == event.task_id {
            return Err(TsqError::new(
                "INVALID_EVENT",
                "task.updated duplicate_of cannot reference itself",
                1,
            )
            .with_details(serde_json::json!({
              "event_id": event_id_value(event),
            })));
        }
        next.duplicate_of = Some(duplicate_of);
    }

    let description = as_string(payload.get("description"));
    let clear_description = as_bool(payload.get("clear_description"));
    if description.is_some() && clear_description == Some(true) {
        return Err(TsqError::new(
            "INVALID_EVENT",
            "task.updated cannot combine description with clear_description",
            1,
        )
        .with_details(serde_json::json!({
          "event_id": event_id_value(event),
        })));
    }
    if let Some(description) = description {
        next.description = Some(description);
    }
    if clear_description == Some(true) {
        next.description = None;
    }

    let external_ref = as_string(payload.get("external_ref"));
    let clear_external_ref = as_bool(payload.get("clear_external_ref"));
    if external_ref.is_some() && clear_external_ref == Some(true) {
        return Err(TsqError::new(
            "INVALID_EVENT",
            "task.updated cannot combine external_ref with clear_external_ref",
            1,
        )
        .with_details(serde_json::json!({
          "event_id": event_id_value(event),
        })));
    }
    if let Some(external_ref) = external_ref {
        next.external_ref = Some(external_ref);
    }
    if clear_external_ref == Some(true) {
        next.external_ref = None;
    }

    if let Some(planning_state) = as_planning_state(payload.get("planning_state")) {
        next.planning_state = Some(planning_state);
    }

    let discovered_from = as_string(payload.get("discovered_from"));
    let clear_discovered_from = as_bool(payload.get("clear_discovered_from"));
    if discovered_from.is_some() && clear_discovered_from == Some(true) {
        return Err(TsqError::new(
            "INVALID_EVENT",
            "task.updated cannot combine discovered_from with clear_discovered_from",
            1,
        )
        .with_details(serde_json::json!({
          "event_id": event_id_value(event),
        })));
    }
    if let Some(ref discovered_from) = discovered_from {
        if discovered_from == &event.task_id {
            return Err(TsqError::new(
                "INVALID_EVENT",
                "task.updated discovered_from cannot reference self",
                1,
            )
            .with_details(serde_json::json!({
              "event_id": event_id_value(event),
            })));
        }
        require_task(state, discovered_from)?;
        next.discovered_from = Some(discovered_from.clone());
    }
    if clear_discovered_from == Some(true) {
        next.discovered_from = None;
    }

    state.tasks.insert(event.task_id.clone(), next);

    Ok(())
}

pub(crate) fn apply_task_status_set(
    state: &mut crate::types::State,
    event: &EventRecord,
) -> Result<(), TsqError> {
    let current = require_task(state, &event.task_id)?.clone();
    let payload = &event.payload;
    let status = as_task_status(payload.get("status"));
    let Some(status) = status else {
        return Err(TsqError::new(
            "INVALID_EVENT",
            "task.status_set requires a valid status",
            1,
        )
        .with_details(serde_json::json!({
          "event_id": event_id_value(event),
        })));
    };

    if matches!(current.status, TaskStatus::Closed | TaskStatus::Canceled)
        && status == TaskStatus::InProgress
    {
        return Err(TsqError::new(
            "INVALID_TRANSITION",
            format!(
                "cannot transition from {} to in_progress",
                task_status_to_string(current.status)
            ),
            1,
        )
        .with_details(serde_json::json!({
          "event_id": event_id_value(event),
          "from": task_status_to_string(current.status),
          "to": task_status_to_string(status),
        })));
    }

    let closed_at = if status == TaskStatus::Closed {
        Some(event.ts.clone())
    } else {
        None
    };
    state.tasks.insert(
        event.task_id.clone(),
        Task {
            status,
            updated_at: event.ts.clone(),
            closed_at,
            ..current
        },
    );

    Ok(())
}

pub(crate) fn apply_task_claimed(
    state: &mut crate::types::State,
    event: &EventRecord,
) -> Result<(), TsqError> {
    let current = require_task(state, &event.task_id)?.clone();
    if matches!(current.status, TaskStatus::Closed | TaskStatus::Canceled) {
        return Err(TsqError::new(
            "INVALID_TRANSITION",
            format!(
                "cannot claim task with status {}",
                task_status_to_string(current.status)
            ),
            1,
        )
        .with_details(serde_json::json!({
          "event_id": event_id_value(event),
          "status": task_status_to_string(current.status),
        })));
    }
    let payload = &event.payload;
    let assignee = as_string(payload.get("assignee")).unwrap_or_else(|| event.actor.clone());
    let next_status = if current.status == TaskStatus::Open {
        TaskStatus::InProgress
    } else {
        current.status
    };
    state.tasks.insert(
        event.task_id.clone(),
        Task {
            assignee: Some(assignee),
            status: next_status,
            updated_at: event.ts.clone(),
            ..current
        },
    );

    Ok(())
}

pub(crate) fn apply_task_noted(
    state: &mut crate::types::State,
    event: &EventRecord,
) -> Result<(), TsqError> {
    let current = require_task(state, &event.task_id)?.clone();
    let payload = &event.payload;
    let text = as_string(payload.get("text"));
    if text
        .as_deref()
        .map(|value| value.is_empty())
        .unwrap_or(true)
    {
        return Err(
            TsqError::new("INVALID_EVENT", "task.noted requires text", 1).with_details(
                serde_json::json!({
                  "event_id": event_id_value(event),
                }),
            ),
        );
    }
    let note = TaskNote {
        event_id: event_identifier(event)?,
        ts: event.ts.clone(),
        actor: event.actor.clone(),
        text: text.unwrap(),
    };
    let mut notes = current.notes.clone();
    notes.push(note);
    state.tasks.insert(
        event.task_id.clone(),
        Task {
            notes,
            updated_at: event.ts.clone(),
            ..current
        },
    );

    Ok(())
}

pub(crate) fn apply_task_spec_attached(
    state: &mut crate::types::State,
    event: &EventRecord,
) -> Result<(), TsqError> {
    let current = require_task(state, &event.task_id)?.clone();
    let payload = &event.payload;
    let spec_path = as_string(payload.get("spec_path"));
    let spec_fingerprint = as_string(payload.get("spec_fingerprint"));
    let spec_attached_at =
        as_string(payload.get("spec_attached_at")).unwrap_or_else(|| event.ts.clone());
    let spec_attached_by =
        as_string(payload.get("spec_attached_by")).unwrap_or_else(|| event.actor.clone());

    if spec_path
        .as_deref()
        .map(|value| value.is_empty())
        .unwrap_or(true)
        || spec_fingerprint
            .as_deref()
            .map(|value| value.is_empty())
            .unwrap_or(true)
    {
        return Err(TsqError::new(
            "INVALID_EVENT",
            "task.spec_attached requires spec_path and spec_fingerprint",
            1,
        )
        .with_details(serde_json::json!({
          "event_id": event_id_value(event),
        })));
    }

    state.tasks.insert(
        event.task_id.clone(),
        Task {
            spec_path: Some(spec_path.unwrap()),
            spec_fingerprint: Some(spec_fingerprint.unwrap()),
            spec_attached_at: Some(spec_attached_at),
            spec_attached_by: Some(spec_attached_by),
            updated_at: event.ts.clone(),
            ..current
        },
    );

    Ok(())
}

pub(crate) fn apply_task_superseded(
    state: &mut crate::types::State,
    event: &EventRecord,
) -> Result<(), TsqError> {
    let source = require_task(state, &event.task_id)?.clone();
    let payload = &event.payload;
    let replacement = as_string(payload.get("with"));
    let Some(replacement) = replacement else {
        return Err(TsqError::new(
            "INVALID_EVENT",
            "task.superseded requires replacement task",
            1,
        )
        .with_details(serde_json::json!({
          "event_id": event_id_value(event),
        })));
    };
    if replacement.is_empty() {
        return Err(TsqError::new(
            "INVALID_EVENT",
            "task.superseded requires replacement task",
            1,
        )
        .with_details(serde_json::json!({
          "event_id": event_id_value(event),
        })));
    }
    if replacement == event.task_id {
        return Err(
            TsqError::new("INVALID_EVENT", "Task cannot supersede itself", 1).with_details(
                serde_json::json!({
                  "task_id": &event.task_id,
                }),
            ),
        );
    }
    require_task(state, &replacement)?;
    let mut next = set_task_closed_state(&source, &event.ts);
    next.superseded_by = Some(replacement);
    state.tasks.insert(event.task_id.clone(), next);

    Ok(())
}
