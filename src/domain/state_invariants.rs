use crate::errors::TsqError;
use crate::types::State;
use std::collections::HashSet;

pub fn validate_projected_state(state: &State) -> Result<(), TsqError> {
    validate_tasks(state)?;
    validate_parent_chains(state)?;
    validate_deps(state)?;
    validate_links(state)?;
    validate_created_order(state)?;
    Ok(())
}

fn validate_tasks(state: &State) -> Result<(), TsqError> {
    for (id, task) in &state.tasks {
        if id.is_empty() || task.id != *id {
            return Err(invalid_state(format!(
                "task map key \"{}\" does not match task id \"{}\"",
                id, task.id
            )));
        }
        if task.priority > 3 {
            return Err(invalid_state(format!(
                "task {} priority {} is outside 0..=3",
                id, task.priority
            )));
        }
        for (field, target) in [
            ("parent_id", task.parent_id.as_ref()),
            ("superseded_by", task.superseded_by.as_ref()),
            ("duplicate_of", task.duplicate_of.as_ref()),
            ("replies_to", task.replies_to.as_ref()),
            ("discovered_from", task.discovered_from.as_ref()),
        ] {
            if let Some(target) = target
                && !state.tasks.contains_key(target)
            {
                return Err(invalid_state(format!(
                    "task {} {} references missing task {}",
                    id, field, target
                )));
            }
        }
    }
    Ok(())
}

fn validate_parent_chains(state: &State) -> Result<(), TsqError> {
    for id in state.tasks.keys() {
        let mut seen = HashSet::new();
        let mut cursor = Some(id.as_str());
        while let Some(current) = cursor {
            if !seen.insert(current) {
                return Err(invalid_state(format!(
                    "parent cycle includes task {}",
                    current
                )));
            }
            cursor = state
                .tasks
                .get(current)
                .and_then(|task| task.parent_id.as_deref());
        }
    }
    Ok(())
}

fn validate_deps(state: &State) -> Result<(), TsqError> {
    for (dependent, blockers) in &state.deps {
        if !state.tasks.contains_key(dependent) {
            return Err(invalid_state(format!(
                "dependency source {} references missing task",
                dependent
            )));
        }
        for blocker in blockers {
            if !state.tasks.contains_key(&blocker.blocker) {
                return Err(invalid_state(format!(
                    "dependency {} blocker {} references missing task",
                    dependent, blocker.blocker
                )));
            }
        }
    }
    Ok(())
}

fn validate_links(state: &State) -> Result<(), TsqError> {
    for (source, by_type) in &state.links {
        if !state.tasks.contains_key(source) {
            return Err(invalid_state(format!(
                "link source {} references missing task",
                source
            )));
        }
        for targets in by_type.values() {
            for target in targets {
                if !state.tasks.contains_key(target) {
                    return Err(invalid_state(format!(
                        "link {} target {} references missing task",
                        source, target
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_created_order(state: &State) -> Result<(), TsqError> {
    let mut order_seen = HashSet::new();
    for id in &state.created_order {
        if !state.tasks.contains_key(id) {
            return Err(invalid_state(format!(
                "created_order references missing task {}",
                id
            )));
        }
        if !order_seen.insert(id.as_str()) {
            return Err(invalid_state(format!(
                "created_order contains duplicate task {}",
                id
            )));
        }
    }
    Ok(())
}

fn invalid_state(message: String) -> TsqError {
    TsqError::new("STATE_INVALID", message, 2)
}
