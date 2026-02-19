use crate::errors::TsqError;
use crate::store::paths::get_paths;
use crate::types::Snapshot;
use chrono::Utc;
use std::fs::{OpenOptions, create_dir_all, read_dir, read_to_string, remove_file, rename};
use std::io::Write;
use std::path::Path;

pub const SNAPSHOT_RETAIN_COUNT: usize = 5;

pub struct LoadedSnapshot {
    pub snapshot: Option<Snapshot>,
    pub warning: Option<String>,
}

fn snapshot_filename(snapshot: &Snapshot) -> String {
    let ts = snapshot.taken_at.replace([':', '.'], "-");
    format!("{}-{}.json", ts, snapshot.event_count)
}

pub fn load_latest_snapshot(repo_root: impl AsRef<Path>) -> Result<Option<Snapshot>, TsqError> {
    let result = load_latest_snapshot_with_warning(repo_root)?;
    Ok(result.snapshot)
}

pub fn load_latest_snapshot_with_warning(
    repo_root: impl AsRef<Path>,
) -> Result<LoadedSnapshot, TsqError> {
    let paths = get_paths(repo_root);
    let entries = match read_dir(&paths.snapshots_dir) {
        Ok(entries) => entries,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return Ok(LoadedSnapshot {
                    snapshot: None,
                    warning: None,
                });
            }
            return Err(
                TsqError::new("SNAPSHOT_READ_FAILED", "Failed listing snapshots", 2)
                    .with_details(io_error_value(&error)),
            );
        }
    };

    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.ends_with(".json") {
                candidates.push(name.to_string());
            }
        }
    }

    if candidates.is_empty() {
        return Ok(LoadedSnapshot {
            snapshot: None,
            warning: None,
        });
    }

    candidates.sort();
    let mut invalid = Vec::new();
    for name in candidates.iter().rev() {
        let candidate = paths.snapshots_dir.join(name);
        match read_to_string(&candidate) {
            Ok(raw) => match serde_json::from_str::<Snapshot>(&raw) {
                Ok(snapshot) => {
                    if is_snapshot(&snapshot) {
                        return Ok(LoadedSnapshot {
                            snapshot: Some(snapshot),
                            warning: if invalid.is_empty() {
                                None
                            } else {
                                Some(invalid_snapshot_warning(&invalid))
                            },
                        });
                    }
                    invalid.push(name.clone());
                }
                Err(_) => invalid.push(name.clone()),
            },
            Err(_) => invalid.push(name.clone()),
        }
    }

    Ok(LoadedSnapshot {
        snapshot: None,
        warning: if invalid.is_empty() {
            None
        } else {
            Some(invalid_snapshot_warning(&invalid))
        },
    })
}

fn is_snapshot(snapshot: &Snapshot) -> bool {
    !snapshot.taken_at.is_empty()
}

fn invalid_snapshot_warning(invalid: &[String]) -> String {
    let first = invalid
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<String>>()
        .join(",");
    let overflow = invalid.len().saturating_sub(3);
    if overflow > 0 {
        format!(
            "Ignored invalid snapshot files: {} (+{} more)",
            first, overflow
        )
    } else {
        format!("Ignored invalid snapshot files: {}", first)
    }
}

fn prune_snapshots(path: &Path) {
    let entries = match read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return;
            }
            let code = error
                .raw_os_error()
                .map(|value| value.to_string())
                .unwrap_or_else(|| format!("{:?}", error.kind()));
            eprintln!("Warning: failed to prune snapshots ({}): {}", code, error);
            return;
        }
    };

    let mut snapshots = Vec::new();
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.ends_with(".json") {
                snapshots.push(name.to_string());
            }
        }
    }

    snapshots.sort();
    if snapshots.len() <= SNAPSHOT_RETAIN_COUNT {
        return;
    }

    let stale = snapshots[..snapshots.len() - SNAPSHOT_RETAIN_COUNT].to_vec();
    for name in stale {
        let _ = remove_file(path.join(name));
    }
}

pub fn write_snapshot(repo_root: impl AsRef<Path>, snapshot: &Snapshot) -> Result<(), TsqError> {
    let paths = get_paths(repo_root);
    create_dir_all(&paths.snapshots_dir).map_err(|error| {
        TsqError::new("SNAPSHOT_WRITE_FAILED", "Failed writing snapshot", 2)
            .with_details(io_error_value(&error))
    })?;

    let target = paths.snapshots_dir.join(snapshot_filename(snapshot));
    let temp = format!(
        "{}.tmp-{}-{}",
        target.display(),
        std::process::id(),
        Utc::now().timestamp_millis()
    );
    let payload = serde_json::to_string_pretty(snapshot).map_err(|error| {
        TsqError::new("SNAPSHOT_WRITE_FAILED", "Failed writing snapshot", 2)
            .with_details(any_error_value(&error))
    })?;

    let mut handle = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temp)
        .map_err(|error| {
            TsqError::new("SNAPSHOT_WRITE_FAILED", "Failed writing snapshot", 2)
                .with_details(io_error_value(&error))
        })?;
    if let Err(error) = handle.write_all(format!("{}\n", payload).as_bytes()) {
        let _ = remove_file(&temp);
        return Err(
            TsqError::new("SNAPSHOT_WRITE_FAILED", "Failed writing snapshot", 2)
                .with_details(io_error_value(&error)),
        );
    }
    if let Err(error) = handle.sync_all() {
        let _ = remove_file(&temp);
        return Err(
            TsqError::new("SNAPSHOT_WRITE_FAILED", "Failed writing snapshot", 2)
                .with_details(io_error_value(&error)),
        );
    }
    if let Err(error) = rename(&temp, &target) {
        let _ = remove_file(&temp);
        return Err(
            TsqError::new("SNAPSHOT_WRITE_FAILED", "Failed writing snapshot", 2)
                .with_details(io_error_value(&error)),
        );
    }
    prune_snapshots(&paths.snapshots_dir);

    Ok(())
}

fn io_error_value(error: &std::io::Error) -> serde_json::Value {
    serde_json::json!({"kind": format!("{:?}", error.kind()), "message": error.to_string()})
}

fn any_error_value(error: &impl std::fmt::Display) -> serde_json::Value {
    serde_json::json!({"message": error.to_string()})
}
