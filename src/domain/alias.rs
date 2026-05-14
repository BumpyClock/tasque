use crate::errors::TsqError;
use crate::types::State;
use std::collections::HashSet;

const MAX_ALIAS_SUFFIX: usize = 1_000_000;

pub fn base_alias(title: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in title.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    let alias = if trimmed.is_empty() {
        "task".to_string()
    } else {
        trimmed
    };
    // Keep generated aliases out of the internal task-id namespace.
    if alias.starts_with("tsq-") {
        format!("task-{}", alias)
    } else {
        alias
    }
}

pub fn allocate_alias(state: &State, title: &str) -> Result<String, TsqError> {
    let taken = taken_aliases_and_ids(state);
    allocate_alias_with_taken(title, &taken)
}

pub fn is_alias_or_id_taken(state: &State, value: &str) -> bool {
    let normalized = value.to_lowercase();
    taken_aliases_and_ids(state).contains(&normalized)
}

pub fn allocate_alias_with_taken(title: &str, taken: &HashSet<String>) -> Result<String, TsqError> {
    let base = base_alias(title);
    if !taken.contains(&base) {
        return Ok(base);
    }
    for suffix in 2..=MAX_ALIAS_SUFFIX {
        let candidate = format!("{}-{}", base, suffix);
        if !taken.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(TsqError::new(
        "ALIAS_COLLISION_LIMIT",
        "alias collision limit exceeded",
        2,
    ))
}

fn taken_aliases_and_ids(state: &State) -> HashSet<String> {
    let mut taken = HashSet::new();
    for task in state.tasks.values() {
        taken.insert(task.id.to_lowercase());
        taken.insert(task.alias.to_lowercase());
    }
    taken
}
