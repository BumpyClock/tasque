use crate::app::service_types::{LabelCount, LabelInput, ServiceContext};
use crate::app::service_utils::{must_resolve_existing, must_task};
use crate::app::storage::{
    append_events, load_projected_state, persist_projection, with_write_lock,
};
use crate::domain::events::make_event;
use crate::domain::labels::{add_label, remove_label};
use crate::domain::projector::apply_events;
use crate::errors::TsqError;
use crate::types::{EventType, Task};
use std::collections::HashMap;

pub fn label_add(ctx: &ServiceContext, input: &LabelInput) -> Result<Task, TsqError> {
    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;
        let existing = must_task(&loaded.state, &id)?;
        let labels = add_label(&existing.labels, &input.label)?;
        let event = make_event(
            &ctx.actor,
            &ctx.now.as_ref()(),
            EventType::TaskUpdated,
            &id,
            serde_json::json!({ "labels": labels })
                .as_object()
                .cloned()
                .unwrap_or_default(),
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

pub fn label_remove(ctx: &ServiceContext, input: &LabelInput) -> Result<Task, TsqError> {
    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;
        let existing = must_task(&loaded.state, &id)?;
        let labels = remove_label(&existing.labels, &input.label)?;
        let event = make_event(
            &ctx.actor,
            &ctx.now.as_ref()(),
            EventType::TaskUpdated,
            &id,
            serde_json::json!({ "labels": labels })
                .as_object()
                .cloned()
                .unwrap_or_default(),
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

pub fn label_list(ctx: &ServiceContext) -> Result<Vec<LabelCount>, TsqError> {
    let loaded = load_projected_state(&ctx.repo_root)?;
    let mut counts: HashMap<String, usize> = HashMap::new();
    for task in loaded.state.tasks.values() {
        for label in &task.labels {
            let count = counts.entry(label.clone()).or_insert(0);
            *count += 1;
        }
    }
    let mut result: Vec<LabelCount> = counts
        .into_iter()
        .map(|(label, count)| LabelCount { label, count })
        .collect();
    result.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(result)
}
