use crate::domain::state_invariants::validate_projected_state;
use crate::errors::TsqError;
use crate::store::atomic::{io_error_value, write_pretty_json_file};
use crate::store::paths::get_paths;
use crate::types::{
    EventLogMetadata, SCHEMA_VERSION, STATE_CACHE_SCHEMA_VERSION, State, StateCache,
};
use std::fs::{create_dir_all, read_to_string};
use std::path::Path;

pub fn write_state_cache(
    repo_root: impl AsRef<Path>,
    state: &State,
    event_log: EventLogMetadata,
) -> Result<(), TsqError> {
    let paths = get_paths(repo_root);
    create_dir_all(&paths.tasque_dir).map_err(|error| {
        TsqError::new("STATE_WRITE_FAILED", "Failed writing state cache", 2)
            .with_details(io_error_value(&error))
    })?;

    let cache = StateCache {
        schema_version: STATE_CACHE_SCHEMA_VERSION,
        event_log: Some(event_log),
        state: state.clone(),
    };
    write_pretty_json_file(
        &paths.state_file,
        &cache,
        "STATE_WRITE_FAILED",
        "Failed writing state cache",
    )
}

pub fn read_state_cache(repo_root: impl AsRef<Path>) -> Result<Option<StateCache>, TsqError> {
    let paths = get_paths(repo_root);
    let primary = paths.state_file;
    let legacy = paths.tasque_dir.join("tasks.jsonl");

    match read_to_string(&primary) {
        Ok(raw) => return parse_state_cache_candidate(&raw, true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(
                TsqError::new("STATE_READ_FAILED", "Failed reading state cache", 2)
                    .with_details(io_error_value(&error)),
            );
        }
    }

    match read_to_string(&legacy) {
        Ok(raw) => parse_state_cache_candidate(&raw, false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(
            TsqError::new("STATE_READ_FAILED", "Failed reading state cache", 2)
                .with_details(io_error_value(&error)),
        ),
    }
}

fn parse_state_cache_candidate(raw: &str, primary: bool) -> Result<Option<StateCache>, TsqError> {
    if let Ok(cache) = serde_json::from_str::<StateCache>(raw) {
        if cache.schema_version == STATE_CACHE_SCHEMA_VERSION
            && validate_projected_state(&cache.state).is_ok()
        {
            return Ok(Some(cache));
        }
        return Ok(None);
    }

    if primary {
        return Ok(None);
    }

    let Ok(state) = serde_json::from_str::<State>(raw) else {
        return Ok(None);
    };
    if validate_projected_state(&state).is_err() {
        return Ok(None);
    }
    Ok(Some(StateCache {
        schema_version: SCHEMA_VERSION,
        event_log: None,
        state,
    }))
}
