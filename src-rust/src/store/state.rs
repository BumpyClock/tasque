use crate::errors::TsqError;
use crate::store::paths::get_paths;
use crate::types::State;
use chrono::Utc;
use std::fs::{OpenOptions, create_dir_all, read_to_string, remove_file, rename};
use std::io::Write;
use std::path::Path;

pub fn write_state_cache(repo_root: impl AsRef<Path>, state: &State) -> Result<(), TsqError> {
    let paths = get_paths(repo_root);
    create_dir_all(&paths.tasque_dir).map_err(|error| {
        TsqError::new("STATE_WRITE_FAILED", "Failed writing state cache", 2)
            .with_details(io_error_value(&error))
    })?;

    let temp = format!(
        "{}.tmp-{}-{}",
        paths.state_file.display(),
        std::process::id(),
        Utc::now().timestamp_millis()
    );
    let payload = serde_json::to_string_pretty(state).map_err(|error| {
        TsqError::new("STATE_WRITE_FAILED", "Failed writing state cache", 2)
            .with_details(any_error_value(&error))
    })?;

    let mut handle = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temp)
        .map_err(|error| {
            TsqError::new("STATE_WRITE_FAILED", "Failed writing state cache", 2)
                .with_details(io_error_value(&error))
        })?;
    if let Err(error) = handle.write_all(format!("{}\n", payload).as_bytes()) {
        let _ = remove_file(&temp);
        return Err(
            TsqError::new("STATE_WRITE_FAILED", "Failed writing state cache", 2)
                .with_details(io_error_value(&error)),
        );
    }
    if let Err(error) = handle.sync_all() {
        let _ = remove_file(&temp);
        return Err(
            TsqError::new("STATE_WRITE_FAILED", "Failed writing state cache", 2)
                .with_details(io_error_value(&error)),
        );
    }
    if let Err(error) = rename(&temp, &paths.state_file) {
        let _ = remove_file(&temp);
        return Err(
            TsqError::new("STATE_WRITE_FAILED", "Failed writing state cache", 2)
                .with_details(io_error_value(&error)),
        );
    }

    Ok(())
}

pub fn read_state_cache(repo_root: impl AsRef<Path>) -> Result<Option<State>, TsqError> {
    let paths = get_paths(repo_root);
    let legacy_state_file = paths.tasque_dir.join("tasks.jsonl");
    let candidates = [paths.state_file, legacy_state_file];

    for state_file in candidates.iter() {
        match read_to_string(state_file) {
            Ok(raw) => match serde_json::from_str::<State>(&raw) {
                Ok(state) => return Ok(Some(state)),
                Err(_) => return Ok(None),
            },
            Err(error) => {
                if error.kind() == std::io::ErrorKind::NotFound {
                    continue;
                }
                return Err(
                    TsqError::new("STATE_READ_FAILED", "Failed reading state cache", 2)
                        .with_details(io_error_value(&error)),
                );
            }
        }
    }

    Ok(None)
}

fn io_error_value(error: &std::io::Error) -> serde_json::Value {
    serde_json::json!({"kind": format!("{:?}", error.kind()), "message": error.to_string()})
}

fn any_error_value(error: &impl std::fmt::Display) -> serde_json::Value {
    serde_json::json!({"message": error.to_string()})
}
