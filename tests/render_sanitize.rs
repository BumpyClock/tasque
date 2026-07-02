mod common;

use common::{create_task_with_args, init_repo, run_cli, run_json};
use serde_json::Value;

// ESC ] 0 ; pwned BEL — an OSC window-title rewrite sequence, the canonical
// terminal-injection payload. `\u{1b}` is ESC (0x1b), `\u{7}` is BEL (0x07).
const EVIL_TITLE: &str = "evil\u{1b}]0;pwned\u{7}title";
const ESC_BYTE: u8 = 0x1b;

fn assert_no_raw_esc(output: &str) {
    // Piped stdout from these commands must never contain a literal,
    // unescaped ESC byte — style.rs disables ANSI color when stdout is not
    // a TTY (see `style::use_color`), so any ESC byte here must have come
    // from user-controlled task data, not from styling.
    assert!(
        !output.as_bytes().contains(&ESC_BYTE),
        "human output contained a raw ESC byte: {:?}",
        output
    );
    assert!(
        output.contains("\\x1b"),
        "expected the escaped form \\x1b to be present: {:?}",
        output
    );
}

#[test]
fn human_show_output_escapes_control_sequences_in_title() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task_with_args(repo.path(), EVIL_TITLE, &["--force"]);

    let result = run_cli(repo.path(), ["show", &id]);

    assert_eq!(result.code, 0, "stderr:\n{}", result.stderr);
    assert_no_raw_esc(&result.stdout);
}

#[test]
fn json_show_output_preserves_raw_control_sequences_in_title() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task_with_args(repo.path(), EVIL_TITLE, &["--force"]);

    let result = run_json(repo.path(), ["show", &id]);

    assert_eq!(result.cli.code, 0, "stderr:\n{}", result.cli.stderr);
    let title = result
        .envelope
        .get("data")
        .and_then(|data| data.get("task"))
        .and_then(|task| task.get("title"))
        .and_then(Value::as_str)
        .expect("expected data.task.title");
    assert_eq!(
        title, EVIL_TITLE,
        "--json must preserve the original bytes for machine consumers"
    );
}

#[test]
fn human_find_output_escapes_control_sequences_in_title() {
    let repo = common::make_repo();
    init_repo(repo.path());
    create_task_with_args(repo.path(), EVIL_TITLE, &["--force"]);

    let result = run_cli(repo.path(), ["find", "open"]);

    assert_eq!(result.code, 0, "stderr:\n{}", result.stderr);
    assert_no_raw_esc(&result.stdout);
}

#[test]
fn human_show_output_escapes_control_sequences_in_description() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let evil_description = format!("body{}", EVIL_TITLE);
    let id = create_task_with_args(
        repo.path(),
        "Task with evil description",
        &["--force", "--description", &evil_description],
    );

    let result = run_cli(repo.path(), ["show", &id]);

    assert_eq!(result.code, 0, "stderr:\n{}", result.stderr);
    assert_no_raw_esc(&result.stdout);
}

#[test]
fn json_show_output_preserves_raw_control_sequences_in_description() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let evil_description = format!("body{}", EVIL_TITLE);
    let id = create_task_with_args(
        repo.path(),
        "Task with evil description",
        &["--force", "--description", &evil_description],
    );

    let result = run_json(repo.path(), ["show", &id]);

    assert_eq!(result.cli.code, 0, "stderr:\n{}", result.cli.stderr);
    let description = result
        .envelope
        .get("data")
        .and_then(|data| data.get("task"))
        .and_then(|task| task.get("description"))
        .and_then(Value::as_str)
        .expect("expected data.task.description");
    assert_eq!(description, evil_description);
}

#[test]
fn human_note_output_escapes_control_sequences() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task_with_args(repo.path(), "Task for note test", &["--force"]);
    let evil_note = format!("note{}", EVIL_TITLE);

    let result = run_cli(repo.path(), ["note", &id, &evil_note]);

    assert_eq!(result.code, 0, "stderr:\n{}", result.stderr);
    assert_no_raw_esc(&result.stdout);

    let list_result = run_cli(repo.path(), ["notes", &id]);
    assert_eq!(list_result.code, 0, "stderr:\n{}", list_result.stderr);
    assert_no_raw_esc(&list_result.stdout);
}

#[test]
fn json_note_output_preserves_raw_control_sequences() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task_with_args(repo.path(), "Task for note json test", &["--force"]);
    let evil_note = format!("note{}", EVIL_TITLE);

    let result = run_json(repo.path(), ["note", &id, &evil_note]);

    assert_eq!(result.cli.code, 0, "stderr:\n{}", result.cli.stderr);
    let text = result
        .envelope
        .get("data")
        .and_then(|data| data.get("note"))
        .and_then(|note| note.get("text"))
        .and_then(Value::as_str)
        .expect("expected data.note.text");
    assert_eq!(text, evil_note);
}

#[test]
fn human_spec_show_output_escapes_control_sequences() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task_with_args(repo.path(), "Task for spec test", &["--force"]);
    let evil_spec = format!("# Context\nbody{}\nmore text", EVIL_TITLE);

    let attach = run_cli(repo.path(), ["spec", &id, "--text", &evil_spec]);
    assert_eq!(attach.code, 0, "stderr:\n{}", attach.stderr);

    let show = run_cli(repo.path(), ["spec", &id, "--show"]);
    assert_eq!(show.code, 0, "stderr:\n{}", show.stderr);
    assert_no_raw_esc(&show.stdout);
    // Real newlines from the spec body must still render as actual line
    // breaks, not be collapsed into a single escaped line.
    assert!(
        show.stdout.contains("# Context\n"),
        "expected real newlines to be preserved in spec content:\n{}",
        show.stdout
    );
}

#[test]
fn json_spec_content_preserves_raw_control_sequences() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task_with_args(repo.path(), "Task for spec json test", &["--force"]);
    let evil_spec = format!("# Context\nbody{}\nmore text", EVIL_TITLE);

    let attach = run_cli(repo.path(), ["spec", &id, "--text", &evil_spec]);
    assert_eq!(attach.code, 0, "stderr:\n{}", attach.stderr);

    let show = run_json(repo.path(), ["spec", &id, "--show"]);
    assert_eq!(show.cli.code, 0, "stderr:\n{}", show.cli.stderr);
    let content = show
        .envelope
        .get("data")
        .and_then(|data| data.get("spec"))
        .and_then(|spec| spec.get("content"))
        .and_then(Value::as_str)
        .expect("expected data.spec.content");
    assert_eq!(content, evil_spec);
}
