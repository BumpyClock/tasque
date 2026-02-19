use crate::errors::TsqError;
use crate::store::paths::get_paths;
use crate::types::{Config, SCHEMA_VERSION};
use chrono::Utc;
use serde_json::Value;
use std::fs::{OpenOptions, create_dir_all, read_to_string, remove_file, rename};
use std::io::Write;
use std::path::Path;

fn is_config(value: &Value) -> Option<Config> {
    let obj = value.as_object()?;
    let schema_version = obj.get("schema_version")?.as_u64()? as u32;
    let snapshot_every = obj.get("snapshot_every")?.as_i64()?;
    if snapshot_every <= 0 {
        return None;
    }
    Some(Config {
        schema_version,
        snapshot_every: snapshot_every as usize,
    })
}

fn default_config() -> Config {
    Config {
        schema_version: SCHEMA_VERSION,
        snapshot_every: 200,
    }
}

pub fn write_default_config(repo_root: impl AsRef<Path>) -> Result<(), TsqError> {
    let paths = get_paths(repo_root);
    create_dir_all(&paths.tasque_dir).map_err(|error| {
        TsqError::new("CONFIG_READ_FAILED", "Failed checking config", 2)
            .with_details(io_error_value(&error))
    })?;

    match read_to_string(&paths.config_file) {
        Ok(_) => return Ok(()),
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err(
                    TsqError::new("CONFIG_READ_FAILED", "Failed checking config", 2)
                        .with_details(io_error_value(&error)),
                );
            }
        }
    }

    let temp = format!(
        "{}.tmp-{}-{}",
        paths.config_file.display(),
        std::process::id(),
        Utc::now().timestamp_millis()
    );
    let payload = serde_json::to_string_pretty(&default_config()).map_err(|error| {
        TsqError::new("CONFIG_WRITE_FAILED", "Failed writing default config", 2)
            .with_details(any_error_value(&error))
    })?;

    let mut handle = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temp)
        .map_err(|error| {
            TsqError::new("CONFIG_WRITE_FAILED", "Failed writing default config", 2)
                .with_details(io_error_value(&error))
        })?;
    if let Err(error) = handle.write_all(format!("{}\n", payload).as_bytes()) {
        let _ = remove_file(&temp);
        return Err(
            TsqError::new("CONFIG_WRITE_FAILED", "Failed writing default config", 2)
                .with_details(io_error_value(&error)),
        );
    }
    if let Err(error) = handle.sync_all() {
        let _ = remove_file(&temp);
        return Err(
            TsqError::new("CONFIG_WRITE_FAILED", "Failed writing default config", 2)
                .with_details(io_error_value(&error)),
        );
    }
    if let Err(error) = rename(&temp, &paths.config_file) {
        let _ = remove_file(&temp);
        return Err(
            TsqError::new("CONFIG_WRITE_FAILED", "Failed writing default config", 2)
                .with_details(io_error_value(&error)),
        );
    }

    Ok(())
}

pub fn read_config(repo_root: impl AsRef<Path>) -> Result<Config, TsqError> {
    let paths = get_paths(repo_root.as_ref());

    let raw = match read_to_string(&paths.config_file) {
        Ok(raw) => raw,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                write_default_config(repo_root)?;
                return Ok(default_config());
            }
            return Err(
                TsqError::new("CONFIG_READ_FAILED", "Failed reading config", 2)
                    .with_details(io_error_value(&error)),
            );
        }
    };

    let parsed: Value = serde_json::from_str(&raw).map_err(|error| {
        TsqError::new("CONFIG_INVALID", "Config JSON is malformed", 2)
            .with_details(any_error_value(&error))
    })?;

    if let Some(config) = is_config(&parsed) {
        return Ok(config);
    }

    Err(TsqError::new("CONFIG_INVALID", "Config shape is invalid", 2).with_details(parsed))
}

fn io_error_value(error: &std::io::Error) -> Value {
    serde_json::json!({"kind": format!("{:?}", error.kind()), "message": error.to_string()})
}

fn any_error_value(error: &impl std::fmt::Display) -> Value {
    serde_json::json!({"message": error.to_string()})
}
