use crate::app::stdin::read_stdin_content;
use crate::errors::TsqError;
use crate::types::Task;
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{OpenOptions, create_dir_all, read_to_string, remove_file, rename};
use std::io::Write;
use std::path::{Path, PathBuf};

pub use crate::app::state::{LoadedState, load_projected_state, persist_projection};
pub use crate::store::config::{read_config, write_default_config};
pub use crate::store::events::{append_events, read_events};
pub use crate::store::lock::with_write_lock;
pub use crate::store::paths::{get_paths, task_spec_file, task_spec_relative_path};
pub use crate::store::snapshots::{load_latest_snapshot, write_snapshot};
pub use crate::store::state::{read_state_cache, write_state_cache};

#[derive(Debug, Clone)]
pub enum SpecAttachSource {
    File { path: String },
    Stdin,
    Text { content: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpecCheckDiagnosticCode {
    SpecNotAttached,
    SpecMetadataInvalid,
    SpecFileMissing,
    SpecFingerprintDrift,
    SpecRequiredSectionsMissing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecCheckDiagnostic {
    pub code: SpecCheckDiagnosticCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecCheckSpec {
    pub attached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<usize>,
    pub required_sections: Vec<String>,
    pub present_sections: Vec<String>,
    pub missing_sections: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecCheckResult {
    pub task_id: String,
    pub ok: bool,
    pub spec: SpecCheckSpec,
    pub diagnostics: Vec<SpecCheckDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecWriteResult {
    pub spec_path: String,
    pub content: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpecAttachInput {
    pub file: Option<String>,
    pub source: Option<String>,
    pub text: Option<String>,
    pub stdin: bool,
}

struct SpecSection {
    label: &'static str,
    aliases: &'static [&'static str],
}

const REQUIRED_SPEC_SECTIONS: &[SpecSection] = &[
    SpecSection {
        label: "Overview",
        aliases: &["Overview"],
    },
    SpecSection {
        label: "Constraints / Non-goals",
        aliases: &["Constraints / Non-goals", "Constraints", "Non-goals"],
    },
    SpecSection {
        label: "Interfaces (CLI/API)",
        aliases: &["Interfaces (CLI/API)", "Interfaces"],
    },
    SpecSection {
        label: "Data model / schema changes",
        aliases: &[
            "Data model / schema changes",
            "Data model",
            "Schema changes",
        ],
    },
    SpecSection {
        label: "Acceptance criteria",
        aliases: &["Acceptance criteria"],
    },
    SpecSection {
        label: "Test plan",
        aliases: &["Test plan"],
    },
];

pub fn ensure_events_file(repo_root: impl AsRef<Path>) -> Result<(), TsqError> {
    let paths = get_paths(repo_root);
    create_dir_all(&paths.tasque_dir).map_err(|error| {
        TsqError::new("IO_ERROR", "failed reading events file", 2)
            .with_details(io_error_value(&error))
    })?;

    match read_to_string(&paths.events_file) {
        Ok(_) => Ok(()),
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err(TsqError::new("IO_ERROR", "failed reading events file", 2)
                    .with_details(io_error_value(&error)));
            }
            let handle = OpenOptions::new()
                .append(true)
                .create(true)
                .open(&paths.events_file)
                .map_err(|error| {
                    TsqError::new("IO_ERROR", "failed reading events file", 2)
                        .with_details(io_error_value(&error))
                })?;
            handle.sync_all().map_err(|error| {
                TsqError::new("IO_ERROR", "failed reading events file", 2)
                    .with_details(io_error_value(&error))
            })?;
            Ok(())
        }
    }
}

pub fn ensure_tasque_gitignore(repo_root: impl AsRef<Path>) -> Result<(), TsqError> {
    let target = get_paths(repo_root).tasque_dir.join(".gitignore");
    let desired = [
        "state.json",
        "state.json.tmp*",
        ".lock",
        "snapshots/",
        "snapshots/*.tmp",
    ];
    std::fs::write(&target, format!("{}\n", desired.join("\n"))).map_err(|error| {
        TsqError::new("IO_ERROR", "failed writing .tasque/.gitignore", 2)
            .with_details(io_error_value(&error))
    })
}

pub fn write_task_spec_atomic(
    repo_root: impl AsRef<Path>,
    task_id: &str,
    content: &str,
) -> Result<SpecWriteResult, TsqError> {
    let spec_file = task_spec_file(repo_root, task_id);
    let spec_path = task_spec_relative_path(task_id);
    if let Some(parent) = spec_file.parent() {
        create_dir_all(parent).map_err(|error| {
            TsqError::new("IO_ERROR", "failed writing attached spec", 2)
                .with_details(io_error_value(&error))
        })?;
    }
    let temp = format!(
        "{}.tmp-{}-{}",
        spec_file.display(),
        std::process::id(),
        Utc::now().timestamp_millis()
    );

    let result = (|| {
        let mut handle = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp)
            .map_err(|error| {
                TsqError::new("IO_ERROR", "failed writing attached spec", 2)
                    .with_details(io_error_value(&error))
            })?;
        handle.write_all(content.as_bytes()).map_err(|error| {
            TsqError::new("IO_ERROR", "failed writing attached spec", 2)
                .with_details(io_error_value(&error))
        })?;
        handle.sync_all().map_err(|error| {
            TsqError::new("IO_ERROR", "failed writing attached spec", 2)
                .with_details(io_error_value(&error))
        })?;
        rename(&temp, &spec_file).map_err(|error| {
            TsqError::new("IO_ERROR", "failed writing attached spec", 2)
                .with_details(io_error_value(&error))
        })?;
        let loaded = read_to_string(&spec_file).map_err(|error| {
            TsqError::new("IO_ERROR", "failed writing attached spec", 2)
                .with_details(io_error_value(&error))
        })?;
        Ok(SpecWriteResult {
            spec_path,
            content: loaded,
        })
    })();

    if result.is_err() {
        let _ = remove_file(&temp);
    }

    result
}

pub fn evaluate_task_spec(
    repo_root: impl AsRef<Path>,
    task_id: &str,
    task: &Task,
) -> Result<SpecCheckResult, TsqError> {
    let spec_path = normalize_optional_input(task.spec_path.as_deref());
    let expected_fingerprint = normalize_optional_input(task.spec_fingerprint.as_deref());
    let required_sections: Vec<String> = REQUIRED_SPEC_SECTIONS
        .iter()
        .map(|section| section.label.to_string())
        .collect();
    let mut diagnostics: Vec<SpecCheckDiagnostic> = Vec::new();
    let mut present_sections: Vec<String> = Vec::new();
    let mut missing_sections = required_sections.clone();
    let mut actual_fingerprint: Option<String> = None;
    let mut bytes: Option<usize> = None;
    let mut content: Option<String> = None;

    if spec_path.is_none() && expected_fingerprint.is_none() {
        diagnostics.push(SpecCheckDiagnostic {
            code: SpecCheckDiagnosticCode::SpecNotAttached,
            message: "task does not have an attached spec".to_string(),
            details: None,
        });
    } else if spec_path.is_none() || expected_fingerprint.is_none() {
        diagnostics.push(SpecCheckDiagnostic {
            code: SpecCheckDiagnosticCode::SpecMetadataInvalid,
            message: "task spec metadata is incomplete".to_string(),
            details: Some(serde_json::json!({
              "has_spec_path": spec_path.is_some(),
              "has_spec_fingerprint": expected_fingerprint.is_some(),
            })),
        });
    }

    if let Some(spec_path_value) = spec_path.clone() {
        let resolved = resolve_spec_path(repo_root, &spec_path_value);
        match read_to_string(&resolved) {
            Ok(value) => content = Some(value),
            Err(error) => {
                if error.kind() == std::io::ErrorKind::NotFound {
                    diagnostics.push(SpecCheckDiagnostic {
                        code: SpecCheckDiagnosticCode::SpecFileMissing,
                        message: "attached spec file not found".to_string(),
                        details: Some(serde_json::json!({"spec_path": spec_path_value})),
                    });
                } else {
                    return Err(TsqError::new(
                        "IO_ERROR",
                        format!("failed reading attached spec file: {}", spec_path_value),
                        2,
                    )
                    .with_details(io_error_value(&error)));
                }
            }
        }
    }

    if let Some(content_value) = content.as_ref() {
        bytes = Some(content_value.len());
        let fingerprint = sha256(content_value);
        actual_fingerprint = Some(fingerprint.clone());
        if let Some(expected) = expected_fingerprint.as_ref()
            && expected != &fingerprint
        {
            diagnostics.push(SpecCheckDiagnostic {
                code: SpecCheckDiagnosticCode::SpecFingerprintDrift,
                message: "spec fingerprint drift detected".to_string(),
                details: Some(serde_json::json!({
                  "expected_fingerprint": expected,
                  "actual_fingerprint": fingerprint,
                })),
            });
        }

        present_sections = extract_markdown_headings(content_value);
        let present_normalized: HashSet<String> = present_sections
            .iter()
            .map(|section| normalize_markdown_heading(section))
            .collect();
        missing_sections = REQUIRED_SPEC_SECTIONS
            .iter()
            .filter(|required| {
                !required
                    .aliases
                    .iter()
                    .any(|alias| present_normalized.contains(&normalize_markdown_heading(alias)))
            })
            .map(|required| required.label.to_string())
            .collect();
        if !missing_sections.is_empty() {
            diagnostics.push(SpecCheckDiagnostic {
                code: SpecCheckDiagnosticCode::SpecRequiredSectionsMissing,
                message: "spec is missing required markdown sections".to_string(),
                details: Some(serde_json::json!({"missing_sections": missing_sections.clone()})),
            });
        }
    }

    Ok(SpecCheckResult {
        task_id: task_id.to_string(),
        ok: diagnostics.is_empty(),
        spec: SpecCheckSpec {
            attached: spec_path.is_some() && expected_fingerprint.is_some(),
            spec_path,
            expected_fingerprint,
            actual_fingerprint,
            bytes,
            required_sections,
            present_sections,
            missing_sections,
        },
        diagnostics,
    })
}

pub fn resolve_spec_attach_source(input: &SpecAttachInput) -> Result<SpecAttachSource, TsqError> {
    let file = normalize_optional_input(input.file.as_deref());
    let positional = normalize_optional_input(input.source.as_deref());
    let has_stdin = input.stdin;
    let has_text = input.text.is_some();

    let sources_provided = [file.is_some(), positional.is_some(), has_stdin, has_text]
        .iter()
        .filter(|value| **value)
        .count();
    if sources_provided != 1 {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "exactly one source is required: --file, --stdin, --text, or positional source path",
            1,
        ));
    }

    if has_text {
        return Ok(SpecAttachSource::Text {
            content: input.text.clone().unwrap_or_default(),
        });
    }
    if has_stdin {
        return Ok(SpecAttachSource::Stdin);
    }
    Ok(SpecAttachSource::File {
        path: file.or(positional).unwrap_or_default(),
    })
}

pub fn read_spec_attach_content(source: &SpecAttachSource) -> Result<String, TsqError> {
    match source {
        SpecAttachSource::Text { content } => Ok(content.clone()),
        SpecAttachSource::Stdin => read_stdin_content(),
        SpecAttachSource::File { path } => read_to_string(path).map_err(|error| {
            TsqError::new(
                "IO_ERROR",
                format!("failed reading spec source file: {}", path),
                2,
            )
            .with_details(io_error_value(&error))
        }),
    }
}

pub fn sha256(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn resolve_spec_path(repo_root: impl AsRef<Path>, spec_path: &str) -> PathBuf {
    let path = PathBuf::from(spec_path);
    if path.is_absolute() {
        return path;
    }
    repo_root.as_ref().join(path)
}

pub fn normalize_optional_input(value: Option<&str>) -> Option<String> {
    let trimmed = value.map(|value| value.trim().to_string());
    match trimmed {
        Some(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

fn extract_markdown_headings(content: &str) -> Vec<String> {
    let regex = Regex::new(r"(?m)^#{1,6}[ \t]+(.+?)\s*$").ok();
    let mut headings = Vec::new();
    let mut seen = HashSet::new();
    let Some(regex) = regex else {
        return headings;
    };
    let trailing = Regex::new(r"[ \t]+#+\s*$").ok();

    for capture in regex.captures_iter(content) {
        let raw = capture.get(1).map(|m| m.as_str()).unwrap_or("");
        let mut heading = raw.trim().to_string();
        if let Some(trailing) = trailing.as_ref() {
            heading = trailing.replace(&heading, "").trim().to_string();
        }
        if heading.is_empty() {
            continue;
        }
        let key = heading.to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);
        headings.push(heading);
    }

    headings
}

fn normalize_markdown_heading(heading: &str) -> String {
    let normalized = heading.trim();
    let whitespace = Regex::new(r"\s+").ok();
    let mut value = match whitespace {
        Some(regex) => regex.replace_all(normalized, " ").to_string(),
        None => normalized.to_string(),
    };
    let trailing_colon = Regex::new(r"\s*:\s*$").ok();
    if let Some(regex) = trailing_colon {
        value = regex.replace(&value, "").to_string();
    }
    value.to_lowercase()
}

fn io_error_value(error: &std::io::Error) -> Value {
    serde_json::json!({"kind": format!("{:?}", error.kind()), "message": error.to_string()})
}
