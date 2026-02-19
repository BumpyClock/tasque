#![allow(dead_code)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use tasque::types::SCHEMA_VERSION;
use tempfile::{Builder, TempDir};

#[derive(Debug)]
pub struct CliOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub struct JsonOutput {
    pub cli: CliOutput,
    pub envelope: Value,
}

pub fn make_repo() -> TempDir {
    Builder::new()
        .prefix("tasque-rust-test-")
        .tempdir()
        .expect("failed creating temporary test repo")
}

pub fn init_repo(repo: &Path) {
    let result = run_cli(repo, ["init"]);
    assert_eq!(
        result.code, 0,
        "expected init success\nstdout:\n{}\nstderr:\n{}",
        result.stdout, result.stderr
    );
}

pub fn create_task(repo: &Path, title: &str) -> String {
    create_task_with_args(repo, title, &[])
}

pub fn create_task_with_args(repo: &Path, title: &str, extra_args: &[&str]) -> String {
    let mut args = vec!["create".to_string(), title.to_string()];
    args.extend(extra_args.iter().map(|value| (*value).to_string()));
    let result = run_json(repo, args);
    assert_eq!(
        result.cli.code, 0,
        "expected create success\nstdout:\n{}\nstderr:\n{}",
        result.cli.stdout, result.cli.stderr
    );
    result
        .envelope
        .get("data")
        .and_then(|value| value.get("task"))
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .expect("create response did not include data.task.id")
}

pub fn update_task(repo: &Path, id: &str, extra_args: &[&str]) -> JsonOutput {
    let mut args = vec!["update".to_string(), id.to_string()];
    args.extend(extra_args.iter().map(|value| (*value).to_string()));
    run_json(repo, args)
}

pub fn label_add(repo: &Path, id: &str, label: &str) -> JsonOutput {
    run_json(repo, ["label", "add", id, label])
}

pub fn assert_envelope_shape(envelope: &Value) {
    assert_eq!(
        envelope.get("schema_version").and_then(Value::as_u64),
        Some(SCHEMA_VERSION as u64)
    );
    assert!(
        envelope.get("command").and_then(Value::as_str).is_some(),
        "envelope.command must be a string"
    );
    let ok = envelope
        .get("ok")
        .and_then(Value::as_bool)
        .expect("envelope.ok must be a boolean");
    if ok {
        assert!(
            envelope.get("data").is_some(),
            "ok envelope must include data"
        );
    } else {
        assert!(
            envelope.get("error").is_some(),
            "error envelope must include error"
        );
    }
}

pub fn assert_validation_error(result: &JsonOutput) {
    assert_eq!(
        result.envelope.get("ok").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|value| value.get("code"))
            .and_then(Value::as_str),
        Some("VALIDATION_ERROR")
    );
}

pub fn ok_data<'a>(envelope: &'a Value) -> &'a Value {
    assert_eq!(envelope.get("ok").and_then(Value::as_bool), Some(true));
    envelope
        .get("data")
        .expect("ok envelope missing data field")
}

pub fn ids_from_task_list(envelope: &Value) -> Vec<String> {
    ok_data(envelope)
        .get("tasks")
        .and_then(Value::as_array)
        .expect("expected data.tasks array")
        .iter()
        .filter_map(|task| task.get("id").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect()
}

pub fn run_cli<I, S>(repo: &Path, args: I) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args_vec = normalize_args(args);
    let output = Command::new(tsq_bin())
        .args(&args_vec)
        .current_dir(repo)
        .env("TSQ_ACTOR", "rust-test")
        .output()
        .expect("failed executing tsq binary");

    CliOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

pub fn run_json<I, S>(repo: &Path, args: I) -> JsonOutput
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args_vec = normalize_args(args);
    args_vec.push("--json".to_string());
    run_json_explicit(repo, args_vec)
}

pub fn run_json_explicit<I, S>(repo: &Path, args: I) -> JsonOutput
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args_vec = normalize_args(args);
    let cli = run_cli(repo, &args_vec);
    let trimmed = cli.stdout.trim();
    assert!(
        !trimmed.is_empty(),
        "expected JSON output but stdout was empty\nstderr:\n{}",
        cli.stderr
    );
    let envelope = serde_json::from_str::<Value>(trimmed).unwrap_or_else(|error| {
        panic!(
            "failed parsing JSON envelope: {error}\nstdout:\n{}\nstderr:\n{}",
            cli.stdout, cli.stderr
        )
    });
    assert_envelope_shape(&envelope);
    JsonOutput { cli, envelope }
}

fn normalize_args<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .map(|value| value.as_ref().to_string())
        .collect()
}

fn tsq_bin() -> PathBuf {
    static BIN_PATH: OnceLock<PathBuf> = OnceLock::new();
    BIN_PATH.get_or_init(resolve_tsq_bin).clone()
}

fn resolve_tsq_bin() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsq") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return candidate;
        }
    }

    let binary_name = if cfg!(windows) { "tsq.exe" } else { "tsq" };
    let debug_candidate = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .and_then(|deps_dir| deps_dir.parent().map(Path::to_path_buf))
        .map(|debug_dir| debug_dir.join(binary_name))
        .filter(|candidate| candidate.exists());
    if let Some(candidate) = debug_candidate {
        return candidate;
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = manifest_dir.join("Cargo.toml");
    let build_status = Command::new("cargo")
        .args([
            "build",
            "--quiet",
            "--manifest-path",
            manifest_path.to_string_lossy().as_ref(),
            "--bin",
            "tsq",
        ])
        .current_dir(&manifest_dir)
        .status()
        .expect("failed to invoke cargo build for tsq test binary");
    assert!(
        build_status.success(),
        "cargo build --bin tsq failed with status: {:?}",
        build_status.code()
    );

    let built_candidate = manifest_dir.join("target").join("debug").join(binary_name);
    assert!(
        built_candidate.exists(),
        "expected built tsq binary at {}",
        built_candidate.display()
    );
    built_candidate
}
