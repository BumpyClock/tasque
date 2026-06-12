use crate::errors::TsqError;
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use std::fs::{OpenOptions, remove_file, rename};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn write_pretty_json_file<T: Serialize>(
    target: &Path,
    value: &T,
    error_code: &'static str,
    message: &'static str,
) -> Result<(), TsqError> {
    let payload = serde_json::to_string_pretty(value).map_err(|error| {
        TsqError::new(error_code, message, 2).with_details(any_error_value(&error))
    })?;
    write_text_file(target, &payload, error_code, message)
}

pub fn write_text_file(
    target: &Path,
    payload: &str,
    error_code: &'static str,
    message: &'static str,
) -> Result<(), TsqError> {
    let temp = format!(
        "{}.tmp-{}-{}-{}",
        target.display(),
        std::process::id(),
        Utc::now().timestamp_millis(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    );

    let result = (|| {
        let mut handle = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp)
            .map_err(|error| {
                TsqError::new(error_code, message, 2).with_details(io_error_value(&error))
            })?;
        handle.write_all(payload.as_bytes()).map_err(|error| {
            TsqError::new(error_code, message, 2).with_details(io_error_value(&error))
        })?;
        handle.write_all(b"\n").map_err(|error| {
            TsqError::new(error_code, message, 2).with_details(io_error_value(&error))
        })?;
        handle.sync_all().map_err(|error| {
            TsqError::new(error_code, message, 2).with_details(io_error_value(&error))
        })?;
        rename(&temp, target).map_err(|error| {
            TsqError::new(error_code, message, 2).with_details(io_error_value(&error))
        })?;
        let parent = target.parent().ok_or_else(|| {
            TsqError::new(error_code, message, 2).with_details(serde_json::json!({
                "message": "target path has no parent directory",
                "target": target.display().to_string(),
            }))
        })?;
        sync_parent_dir(parent, error_code, message)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = remove_file(&temp);
    }
    result
}

#[cfg(unix)]
fn sync_parent_dir(
    parent: &Path,
    error_code: &'static str,
    message: &'static str,
) -> Result<(), TsqError> {
    let dir = OpenOptions::new()
        .read(true)
        .open(parent)
        .map_err(|error| {
            TsqError::new(error_code, message, 2).with_details(io_error_value(&error))
        })?;
    dir.sync_all()
        .map_err(|error| TsqError::new(error_code, message, 2).with_details(io_error_value(&error)))
}

#[cfg(windows)]
fn sync_parent_dir(
    parent: &Path,
    error_code: &'static str,
    message: &'static str,
) -> Result<(), TsqError> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(parent)
        .map_err(|error| {
            TsqError::new(error_code, message, 2).with_details(io_error_value(&error))
        })?;
    dir.sync_all()
        .map_err(|error| TsqError::new(error_code, message, 2).with_details(io_error_value(&error)))
}

#[cfg(not(any(unix, windows)))]
fn sync_parent_dir(
    _parent: &Path,
    _error_code: &'static str,
    _message: &'static str,
) -> Result<(), TsqError> {
    Ok(())
}

pub fn io_error_value(error: &std::io::Error) -> Value {
    serde_json::json!({"kind": format!("{:?}", error.kind()), "message": error.to_string()})
}

pub fn any_error_value(error: &impl std::fmt::Display) -> Value {
    serde_json::json!({"message": error.to_string()})
}
