use super::service_lifecycle_helpers::{duplicate_close_events, payload_map, status_to_string};
use super::service_lifecycle_status::set_lifecycle_status;
use crate::app::service_types::{
    ClaimInput, CloseInput, DuplicateInput, LifecycleStatusInput, ReopenInput, ServiceContext,
    SupersedeInput,
};
use crate::app::service_utils::{
    creates_duplicate_cycle, has_duplicate_link, must_resolve_existing, must_task,
};
use crate::app::state::{load_projected_state, persist_projection};
use crate::app::storage::evaluate_task_spec;
use crate::domain::events::make_event;
use crate::domain::projector::apply_events;
use crate::errors::TsqError;
use crate::store::events::append_events;
use crate::store::lock::with_write_lock;
use crate::types::{EventType, Task, TaskStatus};
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
        let claim_event = make_event(
            &ctx.actor,
            &ctx.now.as_ref()(),
            EventType::TaskClaimed,
            &id,
            payload_map(serde_json::json!({"assignee": assignee})),
        );
        let mut events = vec![claim_event];
        if input.start && existing.status != TaskStatus::InProgress {
            events.push(make_event(
                &ctx.actor,
                &ctx.now.as_ref()(),
                EventType::TaskStatusSet,
                &id,
                payload_map(serde_json::json!({"status": TaskStatus::InProgress})),
            ));
        }
        let mut next_state = apply_events(&loaded.state, &events)?;
        append_events(&ctx.repo_root, &events)?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.event_count + events.len(),
            None,
        )?;
        must_task(&next_state, &id)
    })
}

pub fn close(ctx: &ServiceContext, input: &CloseInput) -> Result<Vec<Task>, TsqError> {
    Ok(set_lifecycle_status(
        ctx,
        &LifecycleStatusInput {
            ids: input.ids.clone(),
            status: TaskStatus::Closed,
            note: None,
            reason: input.reason.clone(),
            exact_id: input.exact_id,
        },
    )?
    .tasks)
}

pub fn reopen(ctx: &ServiceContext, input: &ReopenInput) -> Result<Vec<Task>, TsqError> {
    Ok(set_lifecycle_status(
        ctx,
        &LifecycleStatusInput {
            ids: input.ids.clone(),
            status: TaskStatus::Open,
            note: None,
            reason: None,
            exact_id: input.exact_id,
        },
    )?
    .tasks)
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
            loaded.event_count + 1,
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

        let ts = ctx.now.as_ref()();
        let events = duplicate_close_events(
            &ctx.actor,
            &ts,
            &source,
            &canonical,
            input.reason.as_deref(),
            has_duplicate_link(&loaded.state, &source, &canonical),
        );

        let mut next_state = apply_events(&loaded.state, &events)?;
        append_events(&ctx.repo_root, &events)?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.event_count + events.len(),
            None,
        )?;
        must_task(&next_state, &source)
    })
}
