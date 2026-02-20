#[path = "projector_deps_links.rs"]
mod projector_deps_links;
#[path = "projector_helpers.rs"]
mod projector_helpers;
#[path = "projector_tasks.rs"]
mod projector_tasks;

use crate::errors::TsqError;
use crate::types::{EventRecord, EventType, State};
use projector_deps_links::{
    apply_dep_added, apply_dep_removed, apply_link_added, apply_link_removed,
};
use projector_helpers::{clone_state, event_id_value, event_type_to_string};
use projector_tasks::{
    apply_task_claimed, apply_task_created, apply_task_noted, apply_task_spec_attached,
    apply_task_status_set, apply_task_superseded, apply_task_updated,
};

fn apply_event_mut(state: &mut State, event: &EventRecord) -> Result<(), TsqError> {
    #[allow(unreachable_patterns)]
    match event.event_type {
        EventType::TaskCreated => apply_task_created(state, event)?,
        EventType::TaskUpdated => apply_task_updated(state, event)?,
        EventType::TaskStatusSet => apply_task_status_set(state, event)?,
        EventType::TaskClaimed => apply_task_claimed(state, event)?,
        EventType::TaskNoted => apply_task_noted(state, event)?,
        EventType::TaskSpecAttached => apply_task_spec_attached(state, event)?,
        EventType::TaskSuperseded => apply_task_superseded(state, event)?,
        EventType::DepAdded => apply_dep_added(state, event)?,
        EventType::DepRemoved => apply_dep_removed(state, event)?,
        EventType::LinkAdded => apply_link_added(state, event)?,
        EventType::LinkRemoved => apply_link_removed(state, event)?,
        _ => {
            return Err(
                TsqError::new("INVALID_EVENT_TYPE", "Unknown event type", 1).with_details(
                    serde_json::json!({
                      "event_id": event_id_value(event),
                      "type": event_type_to_string(&event.event_type),
                    }),
                ),
            );
        }
    }
    state.applied_events += 1;
    Ok(())
}

pub fn apply_event(state: &State, event: &EventRecord) -> Result<State, TsqError> {
    let mut next = clone_state(state);
    apply_event_mut(&mut next, event)?;
    Ok(next)
}

pub fn apply_events(base: &State, events: &[EventRecord]) -> Result<State, TsqError> {
    if events.is_empty() {
        return Ok(base.clone());
    }
    let mut state = clone_state(base);
    for event in events {
        apply_event_mut(&mut state, event)?;
    }
    Ok(state)
}
