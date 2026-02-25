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
    let sync_branch = obj
        .get("sync_branch")
        .and_then(Value::as_str)
        .map(String::from);
    Some(Config {
        schema_version,
        snapshot_every: snapshot_every as usize,
        sync_branch,
    })
}

fn default_config() -> Config {
    Config {
        schema_version: SCHEMA_VERSION,
        snapshot_every: 200,
        sync_branch: None,
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

pub fn write_config(repo_root: impl AsRef<Path>, config: &Config) -> Result<(), TsqError> {
    let paths = get_paths(repo_root);
    create_dir_all(&paths.tasque_dir).map_err(|error| {
        TsqError::new("CONFIG_WRITE_FAILED", "Failed writing config", 2)
            .with_details(io_error_value(&error))
    })?;

    let temp = format!(
        "{}.tmp-{}-{}",
        paths.config_file.display(),
        std::process::id(),
        Utc::now().timestamp_millis()
    );
    let payload = serde_json::to_string_pretty(config).map_err(|error| {
        TsqError::new("CONFIG_WRITE_FAILED", "Failed writing config", 2)
            .with_details(any_error_value(&error))
    })?;

    let mut handle = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temp)
        .map_err(|error| {
            TsqError::new("CONFIG_WRITE_FAILED", "Failed writing config", 2)
                .with_details(io_error_value(&error))
        })?;
    if let Err(error) = handle.write_all(format!("{}\n", payload).as_bytes()) {
        let _ = remove_file(&temp);
        return Err(
            TsqError::new("CONFIG_WRITE_FAILED", "Failed writing config", 2)
                .with_details(io_error_value(&error)),
        );
    }
    if let Err(error) = handle.sync_all() {
        let _ = remove_file(&temp);
        return Err(
            TsqError::new("CONFIG_WRITE_FAILED", "Failed writing config", 2)
                .with_details(io_error_value(&error)),
        );
    }
    if let Err(error) = rename(&temp, &paths.config_file) {
        let _ = remove_file(&temp);
        return Err(
            TsqError::new("CONFIG_WRITE_FAILED", "Failed writing config", 2)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn is_config_parses_config_without_sync_branch() {
        let value: Value = serde_json::json!({
            "schema_version": 1,
            "snapshot_every": 200
        });
        let config = is_config(&value).expect("should parse config without sync_branch");
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.snapshot_every, 200);
        assert_eq!(config.sync_branch, None);
    }

    #[test]
    fn is_config_parses_config_with_sync_branch() {
        let value: Value = serde_json::json!({
            "schema_version": 1,
            "snapshot_every": 200,
            "sync_branch": "tasque-sync"
        });
        let config = is_config(&value).expect("should parse config with sync_branch");
        assert_eq!(config.sync_branch, Some("tasque-sync".to_string()));
    }

    #[test]
    fn is_config_ignores_null_sync_branch() {
        let value: Value = serde_json::json!({
            "schema_version": 1,
            "snapshot_every": 200,
            "sync_branch": null
        });
        let config = is_config(&value).expect("should parse config with null sync_branch");
        assert_eq!(config.sync_branch, None);
    }

    #[test]
    fn is_config_rejects_non_string_sync_branch() {
        let value: Value = serde_json::json!({
            "schema_version": 1,
            "snapshot_every": 200,
            "sync_branch": 42
        });
        let config = is_config(&value).expect("should parse config, ignoring non-string sync_branch");
        assert_eq!(config.sync_branch, None);
    }

    #[test]
    fn default_config_has_no_sync_branch() {
        let config = default_config();
        assert_eq!(config.sync_branch, None);
        assert_eq!(config.schema_version, SCHEMA_VERSION);
        assert_eq!(config.snapshot_every, 200);
    }

    #[test]
    fn write_config_roundtrips_with_sync_branch() {
        let dir = TempDir::new().expect("tempdir");
        let repo = dir.path();
        std::fs::create_dir_all(repo.join(".tasque")).expect("mkdir");

        let config = Config {
            schema_version: 1,
            snapshot_every: 100,
            sync_branch: Some("my-sync".to_string()),
        };
        write_config(repo, &config).expect("write_config");

        let loaded = read_config(repo).expect("read_config");
        assert_eq!(loaded, config);
    }

    #[test]
    fn write_config_roundtrips_without_sync_branch() {
        let dir = TempDir::new().expect("tempdir");
        let repo = dir.path();
        std::fs::create_dir_all(repo.join(".tasque")).expect("mkdir");

        let config = Config {
            schema_version: 1,
            snapshot_every: 300,
            sync_branch: None,
        };
        write_config(repo, &config).expect("write_config");

        let loaded = read_config(repo).expect("read_config");
        assert_eq!(loaded, config);
    }

    #[test]
    fn config_serialization_omits_none_sync_branch() {
        let config = Config {
            schema_version: 1,
            snapshot_every: 200,
            sync_branch: None,
        };
        let json = serde_json::to_string(&config).expect("serialize");
        assert!(!json.contains("sync_branch"));
    }

    #[test]
    fn config_serialization_includes_sync_branch_when_set() {
        let config = Config {
            schema_version: 1,
            snapshot_every: 200,
            sync_branch: Some("test-branch".to_string()),
        };
        let json = serde_json::to_string(&config).expect("serialize");
        assert!(json.contains("\"sync_branch\":\"test-branch\""));
    }
}
