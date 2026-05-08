use super::service_lifecycle_helpers::{payload_map, status_to_string};
use crate::app::service_types::{
    LifecycleStatusInput, LifecycleStatusResult, NoteAddResult, ServiceContext,
};
use crate::app::service_utils::{must_resolve_existing, must_task};
use crate::app::storage::{
    append_events, load_projected_state, persist_projection, with_write_lock,
};
use crate::domain::events::make_event;
use crate::domain::projector::apply_events;
use crate::errors::TsqError;
use crate::types::{EventRecord, EventType, Task, TaskStatus};
use serde_json::Value;

pub fn set_lifecycle_status(
    ctx: &ServiceContext,
    input: &LifecycleStatusInput,
) -> Result<LifecycleStatusResult, TsqError> {
    if input.ids.is_empty() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "at least one task id is required",
            1,
        ));
    }

    let note = input
        .note
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string);

    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let resolved_ids: Vec<String> = input
            .ids
            .iter()
            .map(|id| must_resolve_existing(&loaded.state, id, input.exact_id))
            .collect::<Result<_, _>>()?;

        for id in &resolved_ids {
            let task = must_task(&loaded.state, id)?;
            validate_lifecycle_status(id, &task, input.status)?;
        }

        let mut events: Vec<EventRecord> = Vec::with_capacity(
            resolved_ids.len() + note.as_ref().map(|_| resolved_ids.len()).unwrap_or(0),
        );
        for id in &resolved_ids {
            let ts = ctx.now.as_ref()();
            let mut payload = payload_map(serde_json::json!({
                "status": input.status,
                "closed_at": if input.status == TaskStatus::Closed {
                    Some(ts.clone())
                } else {
                    None
                },
            }));
            if input.status == TaskStatus::Closed
                && let Some(reason) = input.reason.as_ref()
            {
                payload.insert("reason".to_string(), Value::String(reason.clone()));
            }
            events.push(make_event(
                &ctx.actor,
                &ts,
                EventType::TaskStatusSet,
                id,
                payload,
            ));
        }
        if let Some(note) = note.as_ref() {
            for id in &resolved_ids {
                events.push(make_event(
                    &ctx.actor,
                    &ctx.now.as_ref()(),
                    EventType::TaskNoted,
                    id,
                    payload_map(serde_json::json!({ "text": note })),
                ));
            }
        }

        let mut next_state = apply_events(&loaded.state, &events)?;
        append_events(&ctx.repo_root, &events)?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.event_count + events.len(),
            None,
        )?;

        let tasks = resolved_ids
            .iter()
            .map(|id| must_task(&next_state, id))
            .collect::<Result<Vec<_>, _>>()?;
        let notes = note_results(&next_state, &events)?;
        Ok(LifecycleStatusResult { tasks, notes })
    })
}

fn validate_lifecycle_status(id: &str, task: &Task, status: TaskStatus) -> Result<(), TsqError> {
    match status {
        TaskStatus::Closed => {
            if task.status == TaskStatus::Closed {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    format!("task {} is already closed", id),
                    1,
                ));
            }
            if task.status == TaskStatus::Canceled {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    format!("cannot close canceled task {}", id),
                    1,
                ));
            }
        }
        TaskStatus::Open => {
            if task.status != TaskStatus::Closed {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    format!(
                        "cannot reopen task {} with status {}",
                        id,
                        status_to_string(task.status)
                    ),
                    1,
                ));
            }
        }
        TaskStatus::InProgress if task.status == TaskStatus::Canceled => {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "cannot move canceled task to in_progress",
                1,
            ));
        }
        _ => {}
    }
    Ok(())
}

fn note_results(
    state: &crate::types::State,
    events: &[EventRecord],
) -> Result<Vec<NoteAddResult>, TsqError> {
    let mut notes = Vec::new();
    for event in events
        .iter()
        .filter(|event| event.event_type == EventType::TaskNoted)
    {
        let event_id = event
            .id
            .as_deref()
            .or(event.event_id.as_deref())
            .ok_or_else(|| TsqError::new("INTERNAL_ERROR", "task note event missing id", 2))?;
        let task = must_task(state, &event.task_id)?;
        let note = task
            .notes
            .iter()
            .find(|note| note.event_id == event_id)
            .cloned()
            .ok_or_else(|| TsqError::new("INTERNAL_ERROR", "task note was not persisted", 2))?;
        notes.push(NoteAddResult {
            task_id: event.task_id.clone(),
            note,
            notes_count: task.notes.len(),
        });
    }
    Ok(notes)
}
