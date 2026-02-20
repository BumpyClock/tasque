use crate::app::service_types::{CreateInput, ServiceContext, UpdateInput};
use crate::app::service_utils::{must_resolve_existing, must_task, unique_root_id};
use crate::app::storage::{
    append_events, load_projected_state, persist_projection, with_write_lock,
};
use crate::domain::events::make_event;
use crate::domain::ids::next_child_id;
use crate::domain::projector::apply_events;
use crate::errors::TsqError;
use crate::types::{EventRecord, EventType, PlanningState, Task, TaskStatus};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{Map, Value};

static EXPLICIT_ID_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^tsq-[0-9a-hjkmnp-tv-z]{8}$").expect("explicit id regex must compile")
});

pub fn create(ctx: &ServiceContext, input: &CreateInput) -> Result<Task, TsqError> {
    if input.explicit_id.is_some() && input.parent.is_some() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --id with --parent",
            1,
        ));
    }
    if input.description.is_some() && input.body_file.is_some() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --description with --body-file",
            1,
        ));
    }

    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;

        let (id, parent_id) = if let Some(explicit_id) = input.explicit_id.as_ref() {
            if !EXPLICIT_ID_PATTERN.is_match(explicit_id) {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "explicit --id must match tsq-<8 crockford base32 chars>",
                    1,
                ));
            }
            if loaded.state.tasks.contains_key(explicit_id) {
                return Err(TsqError::new(
                    "ID_COLLISION",
                    format!("task already exists: {}", explicit_id),
                    1,
                ));
            }
            (explicit_id.clone(), None)
        } else {
            let parent_id = input
                .parent
                .as_ref()
                .map(|raw| must_resolve_existing(&loaded.state, raw, input.exact_id))
                .transpose()?;
            let id = if let Some(parent) = parent_id.as_ref() {
                next_child_id(&loaded.state, parent)
            } else {
                unique_root_id(&loaded.state, &input.title)?
            };
            (id, parent_id)
        };

        let description = if input.body_file.is_some() {
            input.body_file.clone()
        } else {
            input.description.clone()
        };
        let discovered_from = input
            .discovered_from
            .as_ref()
            .map(|raw| must_resolve_existing(&loaded.state, raw, input.exact_id))
            .transpose()?;
        let ts = ctx.now.as_ref()();

        let event = make_event(
            &ctx.actor,
            &ts,
            EventType::TaskCreated,
            &id,
            payload_map(serde_json::json!({
              "id": id,
              "title": input.title,
              "description": description,
              "external_ref": input.external_ref,
              "discovered_from": discovered_from,
              "kind": input.kind,
              "priority": input.priority,
              "status": TaskStatus::Open,
              "parent_id": parent_id,
              "planning_state": input.planning_state.unwrap_or(PlanningState::NeedsPlanning),
            })),
        );

        let mut next_state = apply_events(&loaded.state, std::slice::from_ref(&event))?;
        append_events(&ctx.repo_root, &[event])?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.all_events.len() + 1,
            None,
        )?;
        must_task(&next_state, &id)
    })
}

pub fn update(ctx: &ServiceContext, input: &UpdateInput) -> Result<Task, TsqError> {
    if input.description.is_some() && input.clear_description {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --description with --clear-description",
            1,
        ));
    }
    if input.external_ref.is_some() && input.clear_external_ref {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --external-ref with --clear-external-ref",
            1,
        ));
    }
    if input.discovered_from.is_some() && input.clear_discovered_from {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --discovered-from with --clear-discovered-from",
            1,
        ));
    }

    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;
        let existing = must_task(&loaded.state, &id)?;
        let mut patch = Map::new();

        if let Some(title) = input.title.as_ref() {
            patch.insert("title".to_string(), Value::String(title.clone()));
        }
        if let Some(priority) = input.priority {
            patch.insert("priority".to_string(), serde_json::json!(priority));
        }
        if let Some(description) = input.description.as_ref() {
            patch.insert(
                "description".to_string(),
                Value::String(description.clone()),
            );
        }
        if input.clear_description {
            patch.insert("clear_description".to_string(), Value::Bool(true));
        }
        if let Some(external_ref) = input.external_ref.as_ref() {
            patch.insert(
                "external_ref".to_string(),
                Value::String(external_ref.clone()),
            );
        }
        if input.clear_external_ref {
            patch.insert("clear_external_ref".to_string(), Value::Bool(true));
        }
        if let Some(planning_state) = input.planning_state {
            patch.insert(
                "planning_state".to_string(),
                serde_json::json!(planning_state),
            );
        }
        if let Some(discovered_from_raw) = input.discovered_from.as_ref() {
            let discovered_from =
                must_resolve_existing(&loaded.state, discovered_from_raw, input.exact_id)?;
            patch.insert(
                "discovered_from".to_string(),
                Value::String(discovered_from),
            );
        }
        if input.clear_discovered_from {
            patch.insert("clear_discovered_from".to_string(), Value::Bool(true));
        }

        let has_field_patch = !patch.is_empty();
        let has_status_change = input.status.is_some();
        if !has_field_patch && !has_status_change {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "no update fields provided",
                1,
            ));
        }

        if existing.status == TaskStatus::Canceled && input.status == Some(TaskStatus::InProgress) {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "cannot move canceled task to in_progress",
                1,
            ));
        }

        let mut events: Vec<EventRecord> = Vec::new();
        if has_field_patch {
            events.push(make_event(
                &ctx.actor,
                &ctx.now.as_ref()(),
                EventType::TaskUpdated,
                &id,
                patch,
            ));
        }
        if let Some(status) = input.status {
            let ts = ctx.now.as_ref()();
            let closed_at = if status == TaskStatus::Closed {
                Some(ts.clone())
            } else {
                None
            };
            events.push(make_event(
                &ctx.actor,
                &ts,
                EventType::TaskStatusSet,
                &id,
                payload_map(serde_json::json!({
                  "status": status,
                  "closed_at": closed_at,
                })),
            ));
        }

        let mut next_state = apply_events(&loaded.state, &events)?;
        append_events(&ctx.repo_root, &events)?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.all_events.len() + events.len(),
            None,
        )?;
        must_task(&next_state, &id)
    })
}

fn payload_map(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}
