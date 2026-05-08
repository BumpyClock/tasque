mod common;

use common::{create_task, init_repo, run_cli, run_json, run_json_explicit};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

#[test]
fn note_adds_text_and_notes_lists_it() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Note target");

    let add = run_json(repo.path(), ["note", &id, "needs polish"]);

    assert_eq!(add.cli.code, 0);
    assert_eq!(
        add.envelope["data"]["note"]["text"].as_str(),
        Some("needs polish")
    );

    let list = run_json(repo.path(), ["notes", &id]);

    assert_eq!(list.cli.code, 0);
    assert_eq!(
        list.envelope["data"]["notes"][0]["text"].as_str(),
        Some("needs polish")
    );
}

#[test]
fn note_stdin_rejects_empty_content() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Note stdin target");

    let result = run_cli_with_stdin(repo.path(), ["note", &id, "--stdin", "--json"], "\n");

    assert_eq!(result.cli.code, 1);
    assert_eq!(
        result.envelope["error"]["code"].as_str(),
        Some("VALIDATION_ERROR")
    );
}

#[test]
fn spec_text_attach_and_show_prints_delimited_markdown() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Spec show target");

    let attach = run_json(repo.path(), ["spec", &id, "--text", spec_markdown()]);
    assert_eq!(attach.cli.code, 0);

    let show = run_cli(repo.path(), ["spec", &id, "--show"]);

    assert_eq!(show.code, 0);
    assert!(
        show.stdout
            .contains(&format!("--- spec: .tasque/specs/{id}/spec.md ---")),
        "stdout:\n{}",
        show.stdout
    );
    assert!(show.stdout.contains("## Acceptance criteria"));
    assert!(show.stdout.contains("--- end spec ---"));
}

#[test]
fn spec_show_json_includes_content_path_and_fingerprint() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Spec json target");

    let attach = run_json(repo.path(), ["spec", &id, "--text", spec_markdown()]);
    assert_eq!(attach.cli.code, 0);
    let expected_fingerprint = attach.envelope["data"]["spec"]["spec_fingerprint"]
        .as_str()
        .expect("fingerprint");

    let show = run_json_explicit(repo.path(), ["--format", "json", "spec", &id, "--show"]);

    assert_eq!(show.cli.code, 0);
    let expected_path = format!(".tasque/specs/{id}/spec.md");
    assert_eq!(
        show.envelope["data"]["spec"]["path"].as_str(),
        Some(expected_path.as_str())
    );
    assert_eq!(
        show.envelope["data"]["spec"]["fingerprint"].as_str(),
        Some(expected_fingerprint)
    );
    assert_eq!(
        show.envelope["data"]["spec"]["content"].as_str(),
        Some(spec_markdown())
    );
}

#[test]
fn spec_check_stays_available_on_spec_root() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Spec check target");
    let attach = run_json(repo.path(), ["spec", &id, "--text", spec_markdown()]);
    assert_eq!(attach.cli.code, 0);

    let check = run_json(repo.path(), ["spec", &id, "--check"]);

    assert_eq!(check.cli.code, 0);
    assert_eq!(check.envelope["data"]["ok"].as_bool(), Some(true));
}

#[test]
fn show_with_spec_includes_content_when_requested() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Show with spec target");
    let attach = run_json(repo.path(), ["spec", &id, "--text", spec_markdown()]);
    assert_eq!(attach.cli.code, 0);

    let without_spec = run_json(repo.path(), ["show", &id]);
    assert_eq!(without_spec.cli.code, 0);
    assert!(without_spec.envelope["data"].get("spec").is_none());

    let with_spec = run_json(repo.path(), ["show", &id, "--with-spec"]);

    assert_eq!(with_spec.cli.code, 0);
    assert_eq!(
        with_spec.envelope["data"]["spec"]["content"].as_str(),
        Some(spec_markdown())
    );
}

fn spec_markdown() -> &'static str {
    r#"# Spec

## Overview
Verb-first spec content access.

## Constraints / Non-goals
No storage redesign.

## Interfaces (CLI/API)
Use tsq spec <id> --show.

## Data model / schema changes
No schema changes.

## Acceptance criteria
Content prints with delimiters and JSON content field.

## Test plan
Run cargo test --test verb_first_note_spec --quiet.
"#
}

fn run_cli_with_stdin<I, S>(repo: &Path, args: I, stdin: &str) -> common::JsonOutput
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args_vec = args
        .into_iter()
        .map(|value| value.as_ref().to_string())
        .collect::<Vec<_>>();
    let mut child = Command::new(tsq_bin())
        .args(&args_vec)
        .current_dir(repo)
        .env("TSQ_ACTOR", "rust-test")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed executing tsq binary");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait for tsq");
    let cli = common::CliOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    };
    let envelope = serde_json::from_str::<Value>(cli.stdout.trim()).unwrap_or_else(|error| {
        panic!(
            "failed parsing JSON envelope: {error}\nstdout:\n{}\nstderr:\n{}",
            cli.stdout, cli.stderr
        )
    });
    common::assert_envelope_shape(&envelope);
    common::JsonOutput { cli, envelope }
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
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .and_then(|deps_dir| deps_dir.parent().map(Path::to_path_buf))
        .map(|debug_dir| debug_dir.join(binary_name))
        .filter(|candidate| candidate.exists())
        .expect("failed resolving tsq test binary")
}
