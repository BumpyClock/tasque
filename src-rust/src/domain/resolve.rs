use crate::errors::TsqError;
use crate::types::State;
use serde_json::json;

pub fn resolve_task_id(state: &State, raw: &str, exact_id: bool) -> Result<String, TsqError> {
    if exact_id {
        if state.tasks.contains_key(raw) {
            return Ok(raw.to_string());
        }
        return Err(
            TsqError::new("TASK_NOT_FOUND", "Task ID not found", 1).with_details(json!({
              "input": raw
            })),
        );
    }

    if state.tasks.contains_key(raw) {
        return Ok(raw.to_string());
    }

    let mut matches: Vec<String> = state
        .tasks
        .keys()
        .filter(|task_id| task_id.starts_with(raw))
        .cloned()
        .collect();
    matches.sort();

    if matches.len() == 1 {
        return Ok(matches[0].clone());
    }

    if matches.is_empty() {
        return Err(
            TsqError::new("TASK_NOT_FOUND", "Task ID not found", 1).with_details(json!({
              "input": raw
            })),
        );
    }

    Err(
        TsqError::new("TASK_ID_AMBIGUOUS", "Task ID is ambiguous", 1).with_details(json!({
          "input": raw,
          "candidates": matches
        })),
    )
}
