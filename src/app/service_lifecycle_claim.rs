use super::service_lifecycle_helpers::{payload_map, status_to_string};
use crate::app::service_types::{
    ClaimInput, CloseInput, DuplicateInput, ReopenInput, ServiceContext, SupersedeInput,
};
use crate::app::service_utils::{
    creates_duplicate_cycle, has_duplicate_link, must_resolve_existing, must_task,
};
use crate::app::storage::{
    append_events, evaluate_task_spec, load_projected_state, persist_projection, with_write_lock,
};
use crate::domain::events::make_event;
use crate::domain::projector::apply_events;
use crate::errors::TsqError;
use crate::types::{EventRecord, EventType, RelationType, Task, TaskStatus};
use serde_json::Value;

pub fn claim(ctx: &ServiceContext, input: &ClaimInput) -> Result<Task, TsqError> {
    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;
        let existing = must_task(&loaded.state, &id)?;
        let claimable = matches!(existing.status, TaskStatus::Open | TaskStatus::InProgress);
        if !claimable {
            return Err(TsqError::new(
                "INVALID_STATUS",
                format!(
                    "cannot claim task with status '{}'",
                    status_to_string(existing.status)
                ),
                1,
            ));
        }
        if let Some(assignee) = existing.assignee.as_ref() {
            return Err(TsqError::new(
                "CLAIM_CONFLICT",
                format!("task already assigned to {}", assignee),
                1,
            ));
        }
        if input.require_spec {
            let spec_check = evaluate_task_spec(&ctx.repo_root, &id, &existing)?;
            if !spec_check.ok {
                return Err(TsqError::new(
                    "SPEC_VALIDATION_FAILED",
                    "cannot claim task because required spec check failed",
                    1,
                )
                .with_details(serde_json::json!({
                  "task_id": id,
                  "diagnostics": spec_check.diagnostics,
                })));
            }
        }
        let assignee = input.assignee.clone().unwrap_or_else(|| ctx.actor.clone());
        let event = make_event(
            &ctx.actor,
            &ctx.now.as_ref()(),
            EventType::TaskClaimed,
            &id,
            payload_map(serde_json::json!({"assignee": assignee})),
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

pub fn close(ctx: &ServiceContext, input: &CloseInput) -> Result<Vec<Task>, TsqError> {
    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let resolved_ids: Vec<String> = input
            .ids
            .iter()
            .map(|id| must_resolve_existing(&loaded.state, id, input.exact_id))
            .collect::<Result<_, _>>()?;
        let mut events: Vec<EventRecord> = Vec::new();

        for id in &resolved_ids {
            let existing = must_task(&loaded.state, id)?;
            if existing.status == TaskStatus::Closed {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    format!("task {} is already closed", id),
                    1,
                ));
            }
            if existing.status == TaskStatus::Canceled {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    format!("cannot close canceled task {}", id),
                    1,
                ));
            }
            let ts = ctx.now.as_ref()();
            let mut payload = serde_json::json!({"status": TaskStatus::Closed, "closed_at": ts})
                .as_object()
                .cloned()
                .unwrap_or_default();
            if let Some(reason) = input.reason.as_ref() {
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

        let mut next_state = apply_events(&loaded.state, &events)?;
        append_events(&ctx.repo_root, &events)?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.all_events.len() + events.len(),
            None,
        )?;
        resolved_ids
            .iter()
            .map(|id| must_task(&next_state, id))
            .collect()
    })
}

pub fn reopen(ctx: &ServiceContext, input: &ReopenInput) -> Result<Vec<Task>, TsqError> {
    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let resolved_ids: Vec<String> = input
            .ids
            .iter()
            .map(|id| must_resolve_existing(&loaded.state, id, input.exact_id))
            .collect::<Result<_, _>>()?;
        let mut events: Vec<EventRecord> = Vec::new();

        for id in &resolved_ids {
            let existing = must_task(&loaded.state, id)?;
            if existing.status != TaskStatus::Closed {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    format!(
                        "cannot reopen task {} with status {}",
                        id,
                        status_to_string(existing.status)
                    ),
                    1,
                ));
            }
            events.push(make_event(
                &ctx.actor,
                &ctx.now.as_ref()(),
                EventType::TaskStatusSet,
                id,
                payload_map(serde_json::json!({"status": TaskStatus::Open})),
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
        resolved_ids
            .iter()
            .map(|id| must_task(&next_state, id))
            .collect()
    })
}

pub fn supersede(ctx: &ServiceContext, input: &SupersedeInput) -> Result<Task, TsqError> {
    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let source = must_resolve_existing(&loaded.state, &input.source, input.exact_id)?;
        let with_id = must_resolve_existing(&loaded.state, &input.with_id, input.exact_id)?;
        if source == with_id {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "cannot supersede task with itself",
                1,
            ));
        }
        let mut payload = serde_json::json!({"with": with_id})
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(reason) = input.reason.as_ref() {
            payload.insert("reason".to_string(), Value::String(reason.clone()));
        }
        let event = make_event(
            &ctx.actor,
            &ctx.now.as_ref()(),
            EventType::TaskSuperseded,
            &source,
            payload,
        );
        let mut next_state = apply_events(&loaded.state, std::slice::from_ref(&event))?;
        append_events(&ctx.repo_root, &[event])?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.all_events.len() + 1,
            None,
        )?;
        must_task(&next_state, &source)
    })
}

pub fn duplicate(ctx: &ServiceContext, input: &DuplicateInput) -> Result<Task, TsqError> {
    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let source = must_resolve_existing(&loaded.state, &input.source, input.exact_id)?;
        let canonical = must_resolve_existing(&loaded.state, &input.canonical, input.exact_id)?;
        if source == canonical {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "cannot mark task as duplicate of itself",
                1,
            ));
        }

        let source_task = must_task(&loaded.state, &source)?;
        let canonical_task = must_task(&loaded.state, &canonical)?;
        if source_task.status == TaskStatus::Canceled {
            return Err(TsqError::new(
                "INVALID_STATUS",
                format!("cannot duplicate canceled task {}", source),
                1,
            ));
        }
        if canonical_task.status == TaskStatus::Canceled {
            return Err(TsqError::new(
                "INVALID_STATUS",
                format!("cannot use canceled canonical task {}", canonical),
                1,
            ));
        }
        if let Some(existing) = source_task.duplicate_of.as_ref()
            && existing != &canonical
        {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                format!(
                    "task {} is already marked as duplicate of {}",
                    source, existing
                ),
                1,
            ));
        }
        if creates_duplicate_cycle(&loaded.state, &source, &canonical) {
            return Err(TsqError::new(
                "DUPLICATE_CYCLE",
                format!("duplicate cycle detected: {} -> {}", source, canonical),
                1,
            ));
        }

        let mut events: Vec<EventRecord> = Vec::new();
        if !has_duplicate_link(&loaded.state, &source, &canonical) {
            events.push(make_event(
                &ctx.actor,
                &ctx.now.as_ref()(),
                EventType::LinkAdded,
                &source,
                payload_map(
                    serde_json::json!({"type": RelationType::Duplicates, "target": canonical}),
                ),
            ));
        }

        events.push(make_event(
            &ctx.actor,
            &ctx.now.as_ref()(),
            EventType::TaskUpdated,
            &source,
            payload_map(serde_json::json!({"duplicate_of": canonical})),
        ));

        let ts = ctx.now.as_ref()();
        let mut payload = serde_json::json!({"status": TaskStatus::Closed, "closed_at": ts})
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(reason) = input.reason.as_ref() {
            payload.insert("reason".to_string(), Value::String(reason.clone()));
        }
        events.push(make_event(
            &ctx.actor,
            &ts,
            EventType::TaskStatusSet,
            &source,
            payload,
        ));

        let mut next_state = apply_events(&loaded.state, &events)?;
        append_events(&ctx.repo_root, &events)?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.all_events.len() + events.len(),
            None,
        )?;
        must_task(&next_state, &source)
    })
}
