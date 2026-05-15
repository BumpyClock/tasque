use crate::errors::TsqError;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PathKind {
    Missing,
    File,
    Directory,
    Symlink,
}

pub(super) fn normalize_directory(directory: PathBuf, home: &Path) -> Result<PathBuf, TsqError> {
    let expanded = expand_home(&directory, home);
    if expanded.is_absolute() {
        return Ok(expanded);
    }
    let cwd = env::current_dir().map_err(|e| {
        TsqError::new("IO_ERROR", "failed resolving current directory", 2)
            .with_details(io_error_value(&e))
    })?;
    Ok(cwd.join(expanded))
}

pub(super) fn expand_home(path: &Path, home: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if raw == "~" {
        return home.to_path_buf();
    }
    if raw.starts_with("~/") || raw.starts_with("~\\") {
        return home.join(&raw[2..]);
    }
    path.to_path_buf()
}

pub(super) fn inspect_path(path: &Path) -> Result<PathKind, TsqError> {
    match fs::symlink_metadata(path) {
        Ok(m) => Ok(if m.file_type().is_symlink() {
            PathKind::Symlink
        } else if m.is_dir() {
            PathKind::Directory
        } else {
            PathKind::File
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(PathKind::Missing),
        Err(e) => Err(TsqError::new("IO_ERROR", "failed to inspect skill path", 2)
            .with_details(io_error_value(&e))),
    }
}

pub(super) fn copy_directory_recursive(
    source_directory: &Path,
    destination_directory: &Path,
) -> Result<(), TsqError> {
    fs::create_dir_all(destination_directory).map_err(|e| {
        TsqError::new("IO_ERROR", "failed creating destination skill directory", 2)
            .with_details(io_error_value(&e))
    })?;
    let entries = fs::read_dir(source_directory).map_err(|e| {
        TsqError::new("IO_ERROR", "failed reading source skill directory", 2)
            .with_details(io_error_value(&e))
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| {
            TsqError::new("IO_ERROR", "failed reading source directory entry", 2)
                .with_details(io_error_value(&e))
        })?;
        let source_path = entry.path();
        let destination_path = destination_directory.join(entry.file_name());
        let file_type = entry.file_type().map_err(|e| {
            TsqError::new("IO_ERROR", "failed reading source entry file type", 2)
                .with_details(io_error_value(&e))
        })?;

        if file_type.is_dir() {
            copy_directory_recursive(&source_path, &destination_path)?;
            preserve_permissions(&source_path, &destination_path)?;
            continue;
        }
        if file_type.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|e| {
                TsqError::new("IO_ERROR", "failed copying managed skill file", 2)
                    .with_details(io_error_value(&e))
            })?;
            preserve_permissions(&source_path, &destination_path)?;
            continue;
        }
        if file_type.is_symlink() {
            copy_symlink(&source_path, &destination_path)?;
            continue;
        }
        return Err(TsqError::new(
            "IO_ERROR",
            format!(
                "unsupported entry in managed skill source: {}",
                source_path.display()
            ),
            2,
        ));
    }

    Ok(())
}

#[cfg(unix)]
fn copy_symlink(source_path: &Path, destination_path: &Path) -> Result<(), TsqError> {
    let target = fs::read_link(source_path).map_err(|e| {
        TsqError::new("IO_ERROR", "failed reading source skill symlink", 2)
            .with_details(io_error_value(&e))
    })?;
    std::os::unix::fs::symlink(target, destination_path).map_err(|e| {
        TsqError::new("IO_ERROR", "failed copying managed skill symlink", 2)
            .with_details(io_error_value(&e))
    })
}

#[cfg(windows)]
fn copy_symlink(source_path: &Path, destination_path: &Path) -> Result<(), TsqError> {
    let target = fs::read_link(source_path).map_err(|e| {
        TsqError::new("IO_ERROR", "failed reading source skill symlink", 2)
            .with_details(io_error_value(&e))
    })?;
    let target_path = if target.is_absolute() {
        target.clone()
    } else {
        source_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(&target)
    };

    match fs::metadata(&target_path) {
        Ok(metadata) if metadata.is_dir() => {
            std::os::windows::fs::symlink_dir(&target, destination_path)
        }
        Ok(_) => std::os::windows::fs::symlink_file(&target, destination_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::os::windows::fs::symlink_file(&target, destination_path)
                .or_else(|_| std::os::windows::fs::symlink_dir(&target, destination_path))
        }
        Err(error) => Err(error),
    }
    .map_err(|e| {
        TsqError::new("IO_ERROR", "failed copying managed skill symlink", 2)
            .with_details(io_error_value(&e))
    })
}

fn preserve_permissions(source_path: &Path, destination_path: &Path) -> Result<(), TsqError> {
    let permissions = fs::metadata(source_path)
        .map_err(|e| {
            TsqError::new("IO_ERROR", "failed reading source skill permissions", 2)
                .with_details(io_error_value(&e))
        })?
        .permissions();
    fs::set_permissions(destination_path, permissions).map_err(|e| {
        TsqError::new(
            "IO_ERROR",
            "failed setting destination skill permissions",
            2,
        )
        .with_details(io_error_value(&e))
    })?;
    Ok(())
}

pub(super) fn io_error_value(error: &std::io::Error) -> serde_json::Value {
    serde_json::json!({"kind": error.kind().to_string(), "message": error.to_string()})
}
