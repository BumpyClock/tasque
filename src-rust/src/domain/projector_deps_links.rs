use super::projector_helpers::{
    as_relation_type, as_string, event_id_value, remove_directed_link, require_task,
    upsert_directed_link,
};
use crate::domain::deps::{edge_key, normalize_dependency_edges, normalize_dependency_type};
use crate::domain::validate::assert_no_dependency_cycle;
use crate::errors::TsqError;
use crate::types::{DependencyEdge, DependencyType, EventRecord, RelationType, State};
use serde_json::Value;
use std::collections::HashSet;

const DEFAULT_DEP_TYPE: DependencyType = DependencyType::Blocks;

pub(crate) fn apply_dep_added(state: &mut State, event: &EventRecord) -> Result<(), TsqError> {
    let payload = &event.payload;
    let blocker = as_string(payload.get("blocker"));
    let Some(blocker) = blocker else {
        return Err(
            TsqError::new("INVALID_EVENT", "dep.added requires blocker", 1).with_details(
                serde_json::json!({
                  "event_id": event_id_value(event),
                }),
            ),
        );
    };
    if blocker.is_empty() {
        return Err(
            TsqError::new("INVALID_EVENT", "dep.added requires blocker", 1).with_details(
                serde_json::json!({
                  "event_id": event_id_value(event),
                }),
            ),
        );
    }
    let dep_type = as_string(payload.get("dep_type"))
        .and_then(|value| normalize_dependency_type(value.as_str()))
        .unwrap_or(DEFAULT_DEP_TYPE);
    require_task(state, &event.task_id)?;
    require_task(state, &blocker)?;
    if dep_type == DependencyType::Blocks {
        assert_no_dependency_cycle(state, &event.task_id, &blocker)?;
    }
    let deps = normalize_dependency_edges(state.deps.get(&event.task_id));
    let dep_index: HashSet<String> = deps
        .iter()
        .map(|edge| edge_key(&edge.blocker, edge.dep_type))
        .collect();
    if !dep_index.contains(&edge_key(&blocker, dep_type)) {
        let mut next = deps;
        next.push(DependencyEdge { blocker, dep_type });
        state.deps.insert(event.task_id.clone(), next);
    }

    Ok(())
}

pub(crate) fn apply_dep_removed(state: &mut State, event: &EventRecord) -> Result<(), TsqError> {
    let payload = &event.payload;
    let blocker = as_string(payload.get("blocker"));
    let Some(blocker) = blocker else {
        return Err(
            TsqError::new("INVALID_EVENT", "dep.removed requires blocker", 1).with_details(
                serde_json::json!({
                  "event_id": event_id_value(event),
                }),
            ),
        );
    };
    if blocker.is_empty() {
        return Err(
            TsqError::new("INVALID_EVENT", "dep.removed requires blocker", 1).with_details(
                serde_json::json!({
                  "event_id": event_id_value(event),
                }),
            ),
        );
    }
    let dep_type = as_string(payload.get("dep_type"))
        .and_then(|value| normalize_dependency_type(value.as_str()))
        .unwrap_or(DEFAULT_DEP_TYPE);
    let deps = normalize_dependency_edges(state.deps.get(&event.task_id));
    let next = deps
        .into_iter()
        .filter(|candidate| !(candidate.blocker == blocker && candidate.dep_type == dep_type))
        .collect();
    state.deps.insert(event.task_id.clone(), next);

    Ok(())
}

fn relation_target(payload: &serde_json::Map<String, Value>) -> Option<String> {
    as_string(payload.get("target"))
}

pub(crate) fn apply_link_added(state: &mut State, event: &EventRecord) -> Result<(), TsqError> {
    let payload = &event.payload;
    let rel_type = as_relation_type(payload.get("type"));
    let target = relation_target(payload);
    let (Some(rel_type), Some(target)) = (rel_type, target) else {
        return Err(
            TsqError::new("INVALID_EVENT", "link.added requires target and type", 1).with_details(
                serde_json::json!({
                  "event_id": event_id_value(event),
                }),
            ),
        );
    };
    if target.is_empty() {
        return Err(
            TsqError::new("INVALID_EVENT", "link.added requires target and type", 1).with_details(
                serde_json::json!({
                  "event_id": event_id_value(event),
                }),
            ),
        );
    }
    if target == event.task_id {
        return Err(
            TsqError::new("RELATION_SELF_EDGE", "Relation self-edge is not allowed", 1)
                .with_details(serde_json::json!({
                  "task_id": &event.task_id,
                })),
        );
    }
    require_task(state, &event.task_id)?;
    require_task(state, &target)?;
    upsert_directed_link(&mut state.links, &event.task_id, &target, rel_type);
    if rel_type == RelationType::RelatesTo {
        upsert_directed_link(&mut state.links, &target, &event.task_id, rel_type);
    }

    Ok(())
}

pub(crate) fn apply_link_removed(state: &mut State, event: &EventRecord) -> Result<(), TsqError> {
    let payload = &event.payload;
    let rel_type = as_relation_type(payload.get("type"));
    let target = relation_target(payload);
    let (Some(rel_type), Some(target)) = (rel_type, target) else {
        return Err(
            TsqError::new("INVALID_EVENT", "link.removed requires target and type", 1)
                .with_details(serde_json::json!({
                  "event_id": event_id_value(event),
                })),
        );
    };
    if target.is_empty() {
        return Err(
            TsqError::new("INVALID_EVENT", "link.removed requires target and type", 1)
                .with_details(serde_json::json!({
                  "event_id": event_id_value(event),
                })),
        );
    }
    remove_directed_link(&mut state.links, &event.task_id, &target, rel_type);
    if rel_type == RelationType::RelatesTo {
        remove_directed_link(&mut state.links, &target, &event.task_id, rel_type);
    }

    Ok(())
}
