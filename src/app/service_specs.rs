use super::TasqueService;
use crate::app::service_types::{
    ServiceContext, SpecAttachInput, SpecAttachResult, SpecAttachSpec, SpecCheckInput,
    SpecCheckResult, SpecContentInput, SpecContentResult, SpecPatchInput, SpecUpdateInput,
    SpecUpdateResult, SpecUpdateSpec,
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
use crate::types::{EventRecord, EventType, State, Task};
use diffy::patch_set::{FileOperation, ParseOptions, PatchKind, PatchSet};
use std::path::PathBuf;

impl TasqueService {
    pub fn spec_content(&self, input: SpecContentInput) -> Result<SpecContentResult, TsqError> {
        spec_content(&self.ctx, &input)
    }
}

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
        return Err(empty_spec_error("spec markdown content must not be empty"));
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
        let event = make_spec_attached_event(
            ctx,
            &id,
            &spec_file.spec_path,
            &fingerprint,
            &attached_at,
            &attached_by,
        );
        let mut next_state = apply_events(&loaded.state, std::slice::from_ref(&event))?;
        append_events(&ctx.repo_root, &[event])?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.event_count + 1,
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

pub fn spec_update(
    ctx: &ServiceContext,
    input: &SpecUpdateInput,
) -> Result<SpecUpdateResult, TsqError> {
    let source = resolve_spec_attach_source(&crate::app::storage::SpecAttachInput {
        file: input.file.clone(),
        source: None,
        text: input.text.clone(),
        stdin: input.stdin,
    })?;
    let source_content = read_spec_attach_content(&source)?;
    if source_content.trim().is_empty() {
        return Err(empty_spec_error("spec markdown content must not be empty"));
    }

    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;
        let existing = must_task(&loaded.state, &id)?;
        let attached = validate_attached_spec_current(&ctx.repo_root, &id, &existing)?;
        write_updated_spec(
            ctx,
            &loaded.state,
            loaded.event_count,
            &id,
            &source_content,
            attached.spec_fingerprint,
        )
    })
}

pub fn spec_patch(
    ctx: &ServiceContext,
    input: &SpecPatchInput,
) -> Result<SpecUpdateResult, TsqError> {
    let source = resolve_spec_attach_source(&crate::app::storage::SpecAttachInput {
        file: input.file.clone(),
        source: None,
        text: input.text.clone(),
        stdin: input.stdin,
    })?;
    let patch_content = read_spec_attach_content(&source)?;
    if patch_content.trim().is_empty() {
        return Err(empty_spec_error("spec patch content must not be empty"));
    }

    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;
        let existing = must_task(&loaded.state, &id)?;
        let (attached, current_content) =
            read_current_attached_spec(&ctx.repo_root, &id, &existing)?;
        let updated_content =
            apply_spec_patch(&current_content, &patch_content, &attached.spec_path)?;
        if updated_content.trim().is_empty() {
            return Err(empty_spec_error(
                "patched spec markdown content must not be empty",
            ));
        }
        write_updated_spec(
            ctx,
            &loaded.state,
            loaded.event_count,
            &id,
            &updated_content,
            attached.spec_fingerprint,
        )
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

pub fn spec_content(
    ctx: &ServiceContext,
    input: &SpecContentInput,
) -> Result<SpecContentResult, TsqError> {
    let loaded = load_projected_state(&ctx.repo_root)?;
    let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;
    let task = must_task(&loaded.state, &id)?;
    let attached = require_attached_spec(&task)?;
    let content = read_attached_spec_content(&ctx.repo_root, &id, &attached.spec_path)?;

    Ok(SpecContentResult {
        task_id: id,
        spec_path: attached.spec_path,
        spec_fingerprint: attached.spec_fingerprint,
        content,
    })
}

struct AttachedSpec {
    spec_path: String,
    spec_fingerprint: String,
}

fn require_attached_spec(task: &Task) -> Result<AttachedSpec, TsqError> {
    let spec_path = normalize_optional_input(task.spec_path.as_deref()).ok_or_else(|| {
        TsqError::new(
            "VALIDATION_ERROR",
            format!(
                "task {} has no attached spec; use `tsq spec {} --file spec.md`",
                task.id, task.id
            ),
            1,
        )
    })?;
    let spec_fingerprint =
        normalize_optional_input(task.spec_fingerprint.as_deref()).ok_or_else(|| {
            TsqError::new(
                "VALIDATION_ERROR",
                format!("task {} has no attached spec fingerprint", task.id),
                1,
            )
        })?;
    Ok(AttachedSpec {
        spec_path,
        spec_fingerprint,
    })
}

fn validate_attached_spec_current(
    repo_root: &str,
    task_id: &str,
    task: &Task,
) -> Result<AttachedSpec, TsqError> {
    let (attached, _) = read_current_attached_spec(repo_root, task_id, task)?;
    Ok(attached)
}

fn read_current_attached_spec(
    repo_root: &str,
    task_id: &str,
    task: &Task,
) -> Result<(AttachedSpec, String), TsqError> {
    let attached = require_attached_spec(task)?;
    let content = read_attached_spec_content(repo_root, task_id, &attached.spec_path)?;
    let actual_fingerprint = sha256(&content);
    if actual_fingerprint != attached.spec_fingerprint {
        return Err(TsqError::new(
            "SPEC_CONFLICT",
            format!(
                "attached spec for task {} has drifted from the recorded fingerprint",
                task_id
            ),
            1,
        )
        .with_details(serde_json::json!({
            "task_id": task_id,
            "spec_path": attached.spec_path,
            "expected_fingerprint": attached.spec_fingerprint,
            "actual_fingerprint": actual_fingerprint,
        })));
    }
    Ok((attached, content))
}

fn read_attached_spec_content(
    repo_root: &str,
    task_id: &str,
    spec_path: &str,
) -> Result<String, TsqError> {
    let resolved_path = resolve_spec_path(repo_root, spec_path);
    std::fs::read_to_string(&resolved_path).map_err(|error| {
        let (code, exit_code, message) = if error.kind() == std::io::ErrorKind::NotFound {
            (
                "VALIDATION_ERROR",
                1,
                format!(
                    "attached spec file not found for task {}; use `tsq spec {} --file spec.md`",
                    task_id, task_id
                ),
            )
        } else {
            (
                "IO_ERROR",
                2,
                format!("failed reading attached spec file: {}", spec_path),
            )
        };
        TsqError::new(code, message, exit_code).with_details(serde_json::json!({
            "spec_path": spec_path,
            "message": error.to_string(),
        }))
    })
}

fn write_updated_spec(
    ctx: &ServiceContext,
    state: &State,
    event_count: usize,
    id: &str,
    content: &str,
    old_fingerprint: String,
) -> Result<SpecUpdateResult, TsqError> {
    let spec_file = write_task_spec_atomic(&ctx.repo_root, id, content)?;
    let new_fingerprint = sha256(&spec_file.content);
    let attached_at = ctx.now.as_ref()();
    let attached_by = ctx.actor.clone();
    let event = make_spec_attached_event(
        ctx,
        id,
        &spec_file.spec_path,
        &new_fingerprint,
        &attached_at,
        &attached_by,
    );
    let mut next_state = apply_events(state, std::slice::from_ref(&event))?;
    append_events(&ctx.repo_root, &[event])?;
    persist_projection(&ctx.repo_root, &mut next_state, event_count + 1, None)?;

    Ok(SpecUpdateResult {
        task: must_task(&next_state, id)?,
        spec: SpecUpdateSpec {
            spec_path: spec_file.spec_path,
            old_fingerprint,
            new_fingerprint,
            spec_attached_at: attached_at,
            spec_attached_by: attached_by,
            bytes: spec_file.content.len(),
        },
    })
}

fn make_spec_attached_event(
    ctx: &ServiceContext,
    id: &str,
    spec_path: &str,
    spec_fingerprint: &str,
    spec_attached_at: &str,
    spec_attached_by: &str,
) -> EventRecord {
    make_event(
        &ctx.actor,
        spec_attached_at,
        EventType::TaskSpecAttached,
        id,
        serde_json::json!({
          "spec_path": spec_path,
          "spec_fingerprint": spec_fingerprint,
          "spec_attached_at": spec_attached_at,
          "spec_attached_by": spec_attached_by,
        })
        .as_object()
        .cloned()
        .unwrap_or_default(),
    )
}

fn apply_spec_patch(
    current_content: &str,
    patch_content: &str,
    spec_path: &str,
) -> Result<String, TsqError> {
    let patches = parse_spec_patch_set(patch_content)?;
    if patches.len() != 1 {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "spec patch must contain exactly one file patch",
            1,
        )
        .with_details(serde_json::json!({ "patch_count": patches.len() })));
    }

    let file_patch = patches.into_iter().next().expect("one patch");
    match file_patch.operation() {
        FileOperation::Modify { original, modified } => {
            validate_patch_path(original.as_ref(), spec_path)?;
            validate_patch_path(modified.as_ref(), spec_path)?;
        }
        _ => {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "spec patch must modify the attached spec file",
                1,
            ));
        }
    }
    let patch = match file_patch.patch() {
        PatchKind::Text(patch) => patch,
        PatchKind::Binary(_) => {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "spec patch must be a text patch",
                1,
            ));
        }
    };
    diffy::apply(current_content, patch).map_err(|error| {
        TsqError::new("SPEC_PATCH_FAILED", "spec patch did not apply cleanly", 1)
            .with_details(serde_json::json!({ "message": error.to_string() }))
    })
}

fn parse_spec_patch_set(
    patch_content: &str,
) -> Result<Vec<diffy::patch_set::FilePatch<'_, str>>, TsqError> {
    let parse = |opts| PatchSet::parse(patch_content, opts).collect::<Result<Vec<_>, _>>();
    parse(ParseOptions::unidiff())
        .or_else(|_| parse(ParseOptions::gitdiff()))
        .map_err(|error| {
            TsqError::new("VALIDATION_ERROR", "failed parsing spec patch", 1)
                .with_details(serde_json::json!({"message": error.to_string()}))
        })
}

fn validate_patch_path(path: &str, spec_path: &str) -> Result<(), TsqError> {
    let normalized = normalize_patch_path(path);
    if normalized == spec_path || normalized == "spec.md" {
        return Ok(());
    }
    Err(TsqError::new(
        "VALIDATION_ERROR",
        "spec patch path must match the attached spec file",
        1,
    )
    .with_details(serde_json::json!({
        "patch_path": path,
        "spec_path": spec_path,
    })))
}

fn normalize_patch_path(path: &str) -> String {
    let path = path.trim();
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
        .to_string()
}

fn empty_spec_error(message: &str) -> TsqError {
    TsqError::new("VALIDATION_ERROR", message, 1)
}

fn resolve_spec_path(repo_root: &str, spec_path: &str) -> PathBuf {
    let path = PathBuf::from(spec_path);
    if path.is_absolute() {
        path
    } else {
        PathBuf::from(repo_root).join(path)
    }
}
