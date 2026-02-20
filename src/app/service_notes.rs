use crate::app::service_types::{
    NoteAddInput, NoteAddResult, NoteListInput, NoteListResult, ServiceContext,
};
use crate::app::service_utils::{must_resolve_existing, must_task};
use crate::app::storage::{
    append_events, load_projected_state, persist_projection, with_write_lock,
};
use crate::domain::events::make_event;
use crate::domain::projector::apply_events;
use crate::errors::TsqError;
use crate::types::EventType;

pub fn note_add(ctx: &ServiceContext, input: &NoteAddInput) -> Result<NoteAddResult, TsqError> {
    let text = input.text.trim();
    if text.is_empty() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "note text must not be empty",
            1,
        ));
    }

    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;
        let event = make_event(
            &ctx.actor,
            &ctx.now.as_ref()(),
            EventType::TaskNoted,
            &id,
            serde_json::json!({ "text": text })
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
        let task = must_task(&next_state, &id)?;
        let note = task
            .notes
            .last()
            .cloned()
            .ok_or_else(|| TsqError::new("INTERNAL_ERROR", "task note was not persisted", 2))?;

        Ok(NoteAddResult {
            task_id: id,
            note,
            notes_count: task.notes.len(),
        })
    })
}

pub fn note_list(ctx: &ServiceContext, input: &NoteListInput) -> Result<NoteListResult, TsqError> {
    let loaded = load_projected_state(&ctx.repo_root)?;
    let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;
    let task = must_task(&loaded.state, &id)?;
    Ok(NoteListResult {
        task_id: id,
        notes: task.notes,
    })
}
