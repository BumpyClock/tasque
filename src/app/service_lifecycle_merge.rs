use super::service_lifecycle_helpers::{payload_map, status_to_string};
use crate::app::service_types::{
    DuplicateCandidateGroup, DuplicateCandidatesResult, MergeInput, MergeItem, MergeProjected,
    MergeResult, MergeSummary, MergeTarget, ServiceContext,
};
use crate::app::service_utils::{
    creates_duplicate_cycle, has_duplicate_link, must_resolve_existing, must_task,
    normalize_duplicate_title, sort_tasks,
};
use crate::app::storage::{
    append_events, load_projected_state, persist_projection, with_write_lock,
};
use crate::domain::events::make_event;
use crate::domain::projector::apply_events;
use crate::errors::TsqError;
use crate::types::{EventRecord, EventType, RelationType, Task, TaskStatus};

pub fn merge(ctx: &ServiceContext, input: &MergeInput) -> Result<MergeResult, TsqError> {
    if input.sources.is_empty() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "at least one source task is required",
            1,
        ));
    }

    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;

        let target_id = must_resolve_existing(&loaded.state, &input.into, input.exact_id)?;
        let target_task = must_task(&loaded.state, &target_id)?;
        let mut warnings = Vec::new();

        if matches!(
            target_task.status,
            TaskStatus::Closed | TaskStatus::Canceled
        ) && !input.force
        {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                format!(
                    "target task {} is {}; use --force to merge anyway",
                    target_id,
                    status_to_string(target_task.status)
                ),
                1,
            ));
        }
        if matches!(
            target_task.status,
            TaskStatus::Closed | TaskStatus::Canceled
        ) && input.force
        {
            warnings.push(format!(
                "target {} is {} (forced)",
                target_id,
                status_to_string(target_task.status)
            ));
        }

        let resolved_sources: Vec<String> = input
            .sources
            .iter()
            .map(|source| must_resolve_existing(&loaded.state, source, input.exact_id))
            .collect::<Result<_, _>>()?;

        for source in &resolved_sources {
            if source == &target_id {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    format!("source {} cannot be the same as target", source),
                    1,
                ));
            }
        }

        let mut events: Vec<EventRecord> = Vec::new();
        let mut merged: Vec<MergeItem> = Vec::new();

        for source_id in &resolved_sources {
            let source_task = must_task(&loaded.state, source_id)?;

            if matches!(
                source_task.status,
                TaskStatus::Closed | TaskStatus::Canceled
            ) {
                warnings.push(format!(
                    "{} already {}, skipped",
                    source_id,
                    status_to_string(source_task.status)
                ));
                continue;
            }

            if creates_duplicate_cycle(&loaded.state, source_id, &target_id) {
                warnings.push(format!(
                    "{} -> {} would create a cycle, skipped",
                    source_id, target_id
                ));
                continue;
            }

            if !has_duplicate_link(&loaded.state, source_id, &target_id) {
                events.push(make_event(
                    &ctx.actor,
                    &ctx.now.as_ref()(),
                    EventType::LinkAdded,
                    source_id,
                    payload_map(
                        serde_json::json!({"type": RelationType::Duplicates, "target": target_id}),
                    ),
                ));
            }

            events.push(make_event(
                &ctx.actor,
                &ctx.now.as_ref()(),
                EventType::TaskUpdated,
                source_id,
                payload_map(serde_json::json!({"duplicate_of": target_id})),
            ));

            let ts = ctx.now.as_ref()();
            let mut payload = serde_json::json!({"status": TaskStatus::Closed, "closed_at": ts})
                .as_object()
                .cloned()
                .unwrap_or_default();
            if let Some(reason) = input.reason.as_ref() {
                payload.insert(
                    "reason".to_string(),
                    serde_json::Value::String(reason.clone()),
                );
            }
            events.push(make_event(
                &ctx.actor,
                &ts,
                EventType::TaskStatusSet,
                source_id,
                payload,
            ));

            merged.push(MergeItem {
                id: source_id.to_string(),
                status: "closed".to_string(),
            });
        }

        if input.dry_run {
            let projected_state = if events.is_empty() {
                loaded.state.clone()
            } else {
                apply_events(&loaded.state, &events)?
            };
            let proj_target = must_task(&projected_state, &target_id)?;
            let projected_sources: Vec<Task> = resolved_sources
                .iter()
                .map(|id| must_task(&projected_state, id))
                .collect::<Result<_, _>>()?;
            let merged_sources = merged.len();

            return Ok(MergeResult {
                merged,
                target: MergeTarget {
                    id: target_id.clone(),
                    title: proj_target.title.clone(),
                    status: status_to_string(proj_target.status).to_string(),
                },
                dry_run: true,
                warnings,
                plan_summary: Some(MergeSummary {
                    requested_sources: resolved_sources.len(),
                    merged_sources,
                    skipped_sources: resolved_sources.len() - merged_sources,
                    planned_events: events.len(),
                }),
                projected: Some(MergeProjected {
                    target: proj_target,
                    sources: projected_sources,
                }),
            });
        }

        if !events.is_empty() {
            let mut next_state = apply_events(&loaded.state, &events)?;
            append_events(&ctx.repo_root, &events)?;
            persist_projection(
                &ctx.repo_root,
                &mut next_state,
                loaded.all_events.len() + events.len(),
                None,
            )?;
            let final_target = must_task(&next_state, &target_id)?;
            return Ok(MergeResult {
                merged,
                target: MergeTarget {
                    id: target_id,
                    title: final_target.title,
                    status: status_to_string(final_target.status).to_string(),
                },
                dry_run: false,
                warnings,
                plan_summary: None,
                projected: None,
            });
        }

        Ok(MergeResult {
            merged,
            target: MergeTarget {
                id: target_id,
                title: target_task.title,
                status: status_to_string(target_task.status).to_string(),
            },
            dry_run: false,
            warnings,
            plan_summary: None,
            projected: None,
        })
    })
}

pub fn duplicate_candidates(
    ctx: &ServiceContext,
    limit: usize,
) -> Result<DuplicateCandidatesResult, TsqError> {
    if limit == 0 || limit > 200 {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "limit must be an integer between 1 and 200",
            1,
        ));
    }

    let loaded = load_projected_state(&ctx.repo_root)?;
    let candidates: Vec<Task> = loaded
        .state
        .tasks
        .values()
        .filter(|task| !matches!(task.status, TaskStatus::Closed | TaskStatus::Canceled))
        .filter(|task| task.duplicate_of.is_none())
        .cloned()
        .collect();

    let mut groups: std::collections::HashMap<String, Vec<Task>> = std::collections::HashMap::new();
    for task in &candidates {
        let key = normalize_duplicate_title(&task.title);
        if key.len() < 4 {
            continue;
        }
        groups.entry(key).or_default().push(task.clone());
    }

    let mut grouped: Vec<DuplicateCandidateGroup> = groups
        .into_iter()
        .filter(|(_, tasks)| tasks.len() > 1)
        .map(|(key, tasks)| DuplicateCandidateGroup {
            key,
            tasks: sort_tasks(&tasks),
        })
        .collect();
    grouped.sort_by(|a, b| a.key.cmp(&b.key));
    grouped.truncate(limit);

    Ok(DuplicateCandidatesResult {
        scanned: candidates.len(),
        groups: grouped,
    })
}
