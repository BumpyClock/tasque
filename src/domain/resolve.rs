use crate::errors::TsqError;
use crate::types::State;
use serde_json::json;

pub fn resolve_task_id(state: &State, raw: &str, exact_id: bool) -> Result<String, TsqError> {
    if exact_id {
        if state.tasks.contains_key(raw) {
            return Ok(raw.to_string());
        }
        return Err(not_found(raw));
    }

    if state.tasks.contains_key(raw) {
        return Ok(raw.to_string());
    }

    let raw_alias = raw.to_lowercase();

    let exact_alias_matches = state
        .tasks
        .values()
        .filter(|task| task.alias.to_lowercase() == raw_alias)
        .map(|task| (task.id.clone(), task.alias.clone()))
        .collect();
    if let Some(id) = pick_unique_match(exact_alias_matches, raw)? {
        return Ok(id);
    }

    let id_matches = state
        .tasks
        .values()
        .filter(|task| task.id.starts_with(raw))
        .map(|task| (task.id.clone(), task.alias.clone()))
        .collect();
    if let Some(id) = pick_unique_match(id_matches, raw)? {
        return Ok(id);
    }

    let alias_matches = state
        .tasks
        .values()
        .filter(|task| task.alias.to_lowercase().starts_with(&raw_alias))
        .map(|task| (task.id.clone(), task.alias.clone()))
        .collect();
    pick_unique_match(alias_matches, raw)?.ok_or_else(|| not_found(raw))
}

fn pick_unique_match(
    mut matches: Vec<(String, String)>,
    raw: &str,
) -> Result<Option<String>, TsqError> {
    matches.sort_by(|a, b| a.0.cmp(&b.0));
    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches.remove(0).0)),
        _ => Err(ambiguous(raw, matches)),
    }
}

fn ambiguous(raw: &str, matches: Vec<(String, String)>) -> TsqError {
    TsqError::new("TASK_ID_AMBIGUOUS", "Task ID is ambiguous", 1).with_details(json!({
        "input": raw,
        "candidates": matches
            .into_iter()
            .map(|(id, alias)| json!({ "id": id, "alias": alias }))
            .collect::<Vec<_>>()
    }))
}

fn not_found(raw: &str) -> TsqError {
    TsqError::new("TASK_NOT_FOUND", "Task ID not found", 1).with_details(json!({
      "input": raw
    }))
}
