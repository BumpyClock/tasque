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
    if let Some(task) = state.tasks.values().find(|task| task.alias == raw_alias) {
        return Ok(task.id.clone());
    }

    let mut id_matches: Vec<(String, String)> = state
        .tasks
        .values()
        .filter(|task| task.id.starts_with(raw))
        .map(|task| (task.id.clone(), task.alias.clone()))
        .collect();
    id_matches.sort_by(|a, b| a.0.cmp(&b.0));

    match id_matches.len() {
        1 => return Ok(id_matches[0].0.clone()),
        n if n > 1 => {
            return Err(
                TsqError::new("TASK_ID_AMBIGUOUS", "Task ID is ambiguous", 1).with_details(json!({
                    "input": raw,
                    "candidates": id_matches
                        .into_iter()
                        .map(|(id, alias)| json!({ "id": id, "alias": alias }))
                        .collect::<Vec<_>>()
                })),
            );
        }
        _ => {}
    }

    let mut alias_matches: Vec<(String, String)> = state
        .tasks
        .values()
        .filter(|task| task.alias.starts_with(&raw_alias))
        .map(|task| (task.id.clone(), task.alias.clone()))
        .collect();
    alias_matches.sort_by(|a, b| a.0.cmp(&b.0));

    match alias_matches.len() {
        0 => Err(not_found(raw)),
        1 => Ok(alias_matches[0].0.clone()),
        _ => Err(
            TsqError::new("TASK_ID_AMBIGUOUS", "Task ID is ambiguous", 1).with_details(json!({
                "input": raw,
                "candidates": alias_matches
                    .into_iter()
                    .map(|(id, alias)| json!({ "id": id, "alias": alias }))
                    .collect::<Vec<_>>()
            })),
        ),
    }
}

fn not_found(raw: &str) -> TsqError {
    TsqError::new("TASK_NOT_FOUND", "Task ID not found", 1).with_details(json!({
      "input": raw
    }))
}
