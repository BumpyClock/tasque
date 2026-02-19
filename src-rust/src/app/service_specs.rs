use crate::app::service_types::{
    ServiceContext, SpecAttachInput, SpecAttachResult, SpecAttachSpec, SpecCheckInput,
    SpecCheckResult,
};
use crate::app::service_utils::{must_resolve_existing, must_task};
use crate::app::storage::{
    append_events, evaluate_task_spec, load_projected_state, normalize_optional_input,
    persist_projection, read_spec_attach_content, resolve_spec_attach_source, sha256,
    with_write_lock, write_task_spec_atomic,
};
use crate::domain::events::make_event;
use crate::domain::projector::apply_events;
use crate::errors::TsqError;
use crate::types::EventType;

pub fn spec_attach(
    ctx: &ServiceContext,
    input: &SpecAttachInput,
) -> Result<SpecAttachResult, TsqError> {
    let source = resolve_spec_attach_source(&crate::app::storage::SpecAttachInput {
        file: input.file.clone(),
        source: input.source.clone(),
        text: input.text.clone(),
        stdin: input.stdin,
    })?;
    let source_content = read_spec_attach_content(&source)?;
    if source_content.trim().is_empty() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "spec markdown content must not be empty",
            1,
        ));
    }

    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;
        let existing = must_task(&loaded.state, &id)?;
        let new_fingerprint = sha256(&source_content);
        let old_fingerprint = normalize_optional_input(existing.spec_fingerprint.as_deref());

        if let Some(old) = old_fingerprint
            && old != new_fingerprint
            && !input.force
        {
            return Err(TsqError::new(
                "SPEC_CONFLICT",
                format!(
                    "task {} already has an attached spec with a different fingerprint",
                    id
                ),
                1,
            )
            .with_details(serde_json::json!({
              "task_id": id,
              "old_fingerprint": old,
              "new_fingerprint": new_fingerprint,
            })));
        }

        let spec_file = write_task_spec_atomic(&ctx.repo_root, &id, &source_content)?;
        let fingerprint = sha256(&spec_file.content);
        let attached_at = ctx.now.as_ref()();
        let attached_by = ctx.actor.clone();

        let event = make_event(
            &ctx.actor,
            &attached_at,
            EventType::TaskSpecAttached,
            &id,
            serde_json::json!({
              "spec_path": spec_file.spec_path,
              "spec_fingerprint": fingerprint,
              "spec_attached_at": attached_at,
              "spec_attached_by": attached_by,
            })
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

        Ok(SpecAttachResult {
            task: must_task(&next_state, &id)?,
            spec: SpecAttachSpec {
                spec_path: spec_file.spec_path,
                spec_fingerprint: fingerprint,
                spec_attached_at: attached_at,
                spec_attached_by: attached_by,
                bytes: spec_file.content.len(),
            },
        })
    })
}

pub fn spec_check(
    ctx: &ServiceContext,
    input: &SpecCheckInput,
) -> Result<SpecCheckResult, TsqError> {
    let loaded = load_projected_state(&ctx.repo_root)?;
    let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;
    let task = must_task(&loaded.state, &id)?;
    evaluate_task_spec(&ctx.repo_root, &id, &task)
}
