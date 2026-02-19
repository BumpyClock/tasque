use super::service_lifecycle_helpers::payload_map;
use crate::app::service_types::{DepInput, LinkInput, ServiceContext};
use crate::app::service_utils::must_resolve_existing;
use crate::app::storage::{
    append_events, load_projected_state, persist_projection, with_write_lock,
};
use crate::domain::events::make_event;
use crate::domain::projector::apply_events;
use crate::domain::validate::assert_no_dependency_cycle;
use crate::errors::TsqError;
use crate::types::{DependencyType, EventType, RelationType};

pub fn dep_add(
    ctx: &ServiceContext,
    input: &DepInput,
) -> Result<(String, String, DependencyType), TsqError> {
    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let child = must_resolve_existing(&loaded.state, &input.child, input.exact_id)?;
        let blocker = must_resolve_existing(&loaded.state, &input.blocker, input.exact_id)?;
        let dep_type = input.dep_type.unwrap_or(DependencyType::Blocks);
        if child == blocker {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "task cannot depend on itself",
                1,
            ));
        }
        if dep_type == DependencyType::Blocks {
            assert_no_dependency_cycle(&loaded.state, &child, &blocker)?;
        }
        let event = make_event(
            &ctx.actor,
            &ctx.now.as_ref()(),
            EventType::DepAdded,
            &child,
            payload_map(serde_json::json!({"blocker": blocker, "dep_type": dep_type})),
        );
        let mut next_state = apply_events(&loaded.state, std::slice::from_ref(&event))?;
        append_events(&ctx.repo_root, &[event])?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.all_events.len() + 1,
            None,
        )?;
        Ok((child, blocker, dep_type))
    })
}

pub fn dep_remove(
    ctx: &ServiceContext,
    input: &DepInput,
) -> Result<(String, String, DependencyType), TsqError> {
    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let child = must_resolve_existing(&loaded.state, &input.child, input.exact_id)?;
        let blocker = must_resolve_existing(&loaded.state, &input.blocker, input.exact_id)?;
        let dep_type = input.dep_type.unwrap_or(DependencyType::Blocks);
        let event = make_event(
            &ctx.actor,
            &ctx.now.as_ref()(),
            EventType::DepRemoved,
            &child,
            payload_map(serde_json::json!({"blocker": blocker, "dep_type": dep_type})),
        );
        let mut next_state = apply_events(&loaded.state, std::slice::from_ref(&event))?;
        append_events(&ctx.repo_root, &[event])?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.all_events.len() + 1,
            None,
        )?;
        Ok((child, blocker, dep_type))
    })
}

pub fn link_add(
    ctx: &ServiceContext,
    input: &LinkInput,
) -> Result<(String, String, RelationType), TsqError> {
    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let src = must_resolve_existing(&loaded.state, &input.src, input.exact_id)?;
        let dst = must_resolve_existing(&loaded.state, &input.dst, input.exact_id)?;
        if src == dst {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "self-edge not allowed",
                1,
            ));
        }
        let event = make_event(
            &ctx.actor,
            &ctx.now.as_ref()(),
            EventType::LinkAdded,
            &src,
            payload_map(serde_json::json!({"type": input.rel_type, "target": dst})),
        );
        let mut next_state = apply_events(&loaded.state, std::slice::from_ref(&event))?;
        append_events(&ctx.repo_root, &[event])?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.all_events.len() + 1,
            None,
        )?;
        Ok((src, dst, input.rel_type))
    })
}

pub fn link_remove(
    ctx: &ServiceContext,
    input: &LinkInput,
) -> Result<(String, String, RelationType), TsqError> {
    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let src = must_resolve_existing(&loaded.state, &input.src, input.exact_id)?;
        let dst = must_resolve_existing(&loaded.state, &input.dst, input.exact_id)?;
        if src == dst {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "self-edge not allowed",
                1,
            ));
        }
        let event = make_event(
            &ctx.actor,
            &ctx.now.as_ref()(),
            EventType::LinkRemoved,
            &src,
            payload_map(serde_json::json!({"type": input.rel_type, "target": dst})),
        );
        let mut next_state = apply_events(&loaded.state, std::slice::from_ref(&event))?;
        append_events(&ctx.repo_root, &[event])?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.all_events.len() + 1,
            None,
        )?;
        Ok((src, dst, input.rel_type))
    })
}
