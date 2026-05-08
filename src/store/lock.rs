use crate::errors::TsqError;
use crate::store::paths::get_paths;
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{OpenOptions, create_dir_all, read_to_string, remove_file, rename};
use std::io::Write;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, SystemTime};
use sysinfo::{Pid, System};

const STALE_LOCK_MS: i64 = 30_000;
const JITTER_MIN_MS: u64 = 20;
const JITTER_MAX_MS: u64 = 80;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockPayload {
    pub host: String,
    pub pid: u32,
    pub created_at: String,
}

fn lock_timeout_ms() -> u64 {
    std::env::var("TSQ_LOCK_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(3_000)
}

fn jitter_ms() -> u64 {
    rand::thread_rng().gen_range(JITTER_MIN_MS..=JITTER_MAX_MS)
}

fn parse_lock_payload(raw: &str) -> Option<LockPayload> {
    serde_json::from_str(raw).ok()
}

fn is_process_dead(pid: u32) -> bool {
    if pid == std::process::id() {
        return false;
    }
    let mut system = System::new();
    system.refresh_processes();
    system.process(Pid::from_u32(pid)).is_none()
}

fn try_cleanup_stale_lock(lock_file: &Path, current_host: &str) -> bool {
    let raw = match read_to_string(lock_file) {
        Ok(raw) => raw,
        Err(error) => return error.kind() == std::io::ErrorKind::NotFound,
    };

    let payload = match parse_lock_payload(&raw) {
        Some(payload) => payload,
        None => return false,
    };

    if payload.host != current_host {
        return false;
    }

    let created_at = match DateTime::parse_from_rfc3339(&payload.created_at) {
        Ok(value) => value.with_timezone(&Utc),
        Err(_) => return false,
    };

    let now = Utc::now();
    if now.timestamp_millis() - created_at.timestamp_millis() < STALE_LOCK_MS {
        return false;
    }

    if !is_process_dead(payload.pid) {
        return false;
    }

    let mut suffix = String::new();
    for _ in 0..4 {
        suffix.push_str(&format!("{:02x}", rand::thread_rng().r#gen::<u8>()));
    }
    let temp_file = format!("{}.stale-{}", lock_file.display(), suffix);
    if let Err(error) = rename(lock_file, &temp_file) {
        if error.kind() == std::io::ErrorKind::NotFound {
            return true;
        }
        return false;
    }

    let moved_raw = match read_to_string(&temp_file) {
        Ok(raw) => raw,
        Err(_) => return false,
    };

    if moved_raw != raw {
        let _ = rename(&temp_file, lock_file);
        return false;
    }

    let _ = remove_file(&temp_file);
    true
}

fn acquire_write_lock(lock_file: &Path, tasque_dir: &Path) -> Result<LockPayload, TsqError> {
    acquire_write_lock_with_timeout(lock_file, tasque_dir, lock_timeout_ms())
}

fn acquire_write_lock_with_timeout(
    lock_file: &Path,
    tasque_dir: &Path,
    timeout_ms: u64,
) -> Result<LockPayload, TsqError> {
    let deadline = SystemTime::now()
        .checked_add(Duration::from_millis(timeout_ms))
        .unwrap_or(SystemTime::now());
    let host = System::host_name().unwrap_or_else(|| "unknown".to_string());

    loop {
        let payload = LockPayload {
            host: host.clone(),
            pid: std::process::id(),
            created_at: Utc::now().to_rfc3339(),
        };

        if let Err(error) = create_dir_all(tasque_dir) {
            return Err(
                TsqError::new("LOCK_ACQUIRE_FAILED", "Failed to acquire write lock", 2)
                    .with_details(io_error_value(&error)),
            );
        }

        let open_result = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_file);
        match open_result {
            Ok(mut handle) => {
                let payload_json = serde_json::to_string(&payload).map_err(|error| {
                    TsqError::new("LOCK_ACQUIRE_FAILED", "Failed to acquire write lock", 2)
                        .with_details(any_error_value(&error))
                })?;
                if let Err(error) = handle.write_all(format!("{}\n", payload_json).as_bytes()) {
                    return Err(TsqError::new(
                        "LOCK_ACQUIRE_FAILED",
                        "Failed to acquire write lock",
                        2,
                    )
                    .with_details(io_error_value(&error)));
                }
                if let Err(error) = handle.sync_all() {
                    return Err(TsqError::new(
                        "LOCK_ACQUIRE_FAILED",
                        "Failed to acquire write lock",
                        2,
                    )
                    .with_details(io_error_value(&error)));
                }
                return Ok(payload);
            }
            Err(error) => {
                let retryable = matches!(
                    error.kind(),
                    std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::PermissionDenied
                );
                if !retryable {
                    return Err(TsqError::new(
                        "LOCK_ACQUIRE_FAILED",
                        "Failed to acquire write lock",
                        2,
                    )
                    .with_details(io_error_value(&error)));
                }
            }
        }

        if try_cleanup_stale_lock(lock_file, &host) {
            continue;
        }

        if SystemTime::now() >= deadline {
            return Err(
                TsqError::new("LOCK_TIMEOUT", "Timed out acquiring write lock", 3).with_details(
                    serde_json::json!({
                      "lockFile": lock_file.display().to_string(),
                      "timeout_ms": timeout_ms,
                    }),
                ),
            );
        }

        sleep(Duration::from_millis(jitter_ms()));
    }
}

fn release_write_lock(lock_file: &Path, owned: &LockPayload) -> Result<(), TsqError> {
    let raw = match read_to_string(lock_file) {
        Ok(raw) => raw,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return Ok(());
            }
            return Err(TsqError::new(
                "LOCK_RELEASE_FAILED",
                "Failed reading lock file on release",
                2,
            )
            .with_details(io_error_value(&error)));
        }
    };

    let payload = match parse_lock_payload(&raw) {
        Some(payload) => payload,
        None => return Ok(()),
    };

    if payload.host != owned.host
        || payload.pid != owned.pid
        || payload.created_at != owned.created_at
    {
        return Err(TsqError::new(
            "LOCK_OWNERSHIP_MISMATCH",
            "Lock file is owned by another writer",
            2,
        )
        .with_details(serde_json::json!({
            "lockFile": lock_file.display().to_string(),
            "owner": payload,
            "attempted_owner": owned,
        })));
    }

    match remove_file(lock_file) {
        Ok(_) => Ok(()),
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return Ok(());
            }
            Err(
                TsqError::new("LOCK_RELEASE_FAILED", "Failed removing lock file", 2)
                    .with_details(io_error_value(&error)),
            )
        }
    }
}

pub fn force_remove_lock(repo_root: impl AsRef<Path>) -> Result<Option<LockPayload>, TsqError> {
    let paths = get_paths(repo_root);
    let raw = match read_to_string(&paths.lock_file) {
        Ok(raw) => raw,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(
                TsqError::new("LOCK_REMOVE_FAILED", "Failed reading lock file", 2)
                    .with_details(io_error_value(&error)),
            );
        }
    };

    let payload = parse_lock_payload(&raw);
    if let Err(error) = remove_file(&paths.lock_file)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(
            TsqError::new("LOCK_REMOVE_FAILED", "Failed removing lock file", 2)
                .with_details(io_error_value(&error)),
        );
    }

    Ok(payload)
}

pub fn lock_exists(repo_root: impl AsRef<Path>) -> Result<bool, TsqError> {
    let paths = get_paths(repo_root);
    match read_to_string(&paths.lock_file) {
        Ok(_) => Ok(true),
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return Ok(false);
            }
            let code = error
                .raw_os_error()
                .map(|value| value.to_string())
                .unwrap_or_else(|| format!("{:?}", error.kind()));
            Err(TsqError::new(
                "LOCK_CHECK_FAILED",
                format!("Failed checking lock file: {}", code),
                2,
            )
            .with_details(io_error_value(&error)))
        }
    }
}

pub fn with_write_lock<T, F>(repo_root: impl AsRef<Path>, f: F) -> Result<T, TsqError>
where
    F: FnOnce() -> Result<T, TsqError>,
{
    let paths = get_paths(repo_root);
    let lock = acquire_write_lock(&paths.lock_file, &paths.tasque_dir)?;

    let result = f();
    match result {
        Ok(value) => {
            release_write_lock(&paths.lock_file, &lock)?;
            Ok(value)
        }
        Err(error) => {
            if release_write_lock(&paths.lock_file, &lock).is_err() {
                return Err(TsqError::new(
                    "INTERNAL_ERROR",
                    "Both callback and lock release failed",
                    2,
                ));
            }
            Err(error)
        }
    }
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
    use crate::store::paths::get_paths;
    use tempfile::TempDir;

    fn write_lock(path: &Path, payload: &LockPayload) {
        std::fs::write(
            path,
            format!(
                "{}\n",
                serde_json::to_string(payload).expect("serialize lock")
            ),
        )
        .expect("write lock");
    }

    #[test]
    fn live_lock_times_out() {
        let dir = TempDir::new().expect("tempdir");
        let paths = get_paths(dir.path());
        std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");
        let payload = LockPayload {
            host: System::host_name().unwrap_or_else(|| "unknown".to_string()),
            pid: std::process::id(),
            created_at: Utc::now().to_rfc3339(),
        };
        write_lock(&paths.lock_file, &payload);

        let err = acquire_write_lock_with_timeout(&paths.lock_file, &paths.tasque_dir, 1)
            .expect_err("live lock should time out");

        assert_eq!(err.code, "LOCK_TIMEOUT");
    }

    #[test]
    fn same_host_dead_pid_stale_lock_is_removed() {
        let dir = TempDir::new().expect("tempdir");
        let paths = get_paths(dir.path());
        std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");
        let payload = LockPayload {
            host: System::host_name().unwrap_or_else(|| "unknown".to_string()),
            pid: u32::MAX,
            created_at: (Utc::now() - chrono::Duration::milliseconds(STALE_LOCK_MS + 1_000))
                .to_rfc3339(),
        };
        write_lock(&paths.lock_file, &payload);

        let owned = acquire_write_lock_with_timeout(&paths.lock_file, &paths.tasque_dir, 200)
            .expect("stale lock should be replaced");

        assert_eq!(owned.pid, std::process::id());
        release_write_lock(&paths.lock_file, &owned).expect("release owned lock");
    }

    #[test]
    fn release_wrong_owner_reports_error_and_keeps_lock() {
        let dir = TempDir::new().expect("tempdir");
        let paths = get_paths(dir.path());
        std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");
        let owned = LockPayload {
            host: "host-a".to_string(),
            pid: 1,
            created_at: "2026-05-08T00:00:00Z".to_string(),
        };
        let other = LockPayload {
            host: "host-b".to_string(),
            pid: 2,
            created_at: "2026-05-08T00:00:01Z".to_string(),
        };
        write_lock(&paths.lock_file, &other);

        let err =
            release_write_lock(&paths.lock_file, &owned).expect_err("wrong owner should fail");

        assert_eq!(err.code, "LOCK_OWNERSHIP_MISMATCH");
        let raw = std::fs::read_to_string(&paths.lock_file).expect("lock still exists");
        assert!(raw.contains("host-b"));
    }

    #[test]
    fn with_write_lock_releases_lock_when_callback_errors() {
        let dir = TempDir::new().expect("tempdir");
        let repo = dir.path();
        let paths = get_paths(repo);

        let err = with_write_lock(repo, || {
            Err::<(), TsqError>(TsqError::new("CALLBACK_FAILED", "callback failed", 2))
        })
        .expect_err("callback should fail");

        assert_eq!(err.code, "CALLBACK_FAILED");
        assert!(!paths.lock_file.exists());
    }
}
