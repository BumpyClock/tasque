mod common;

use common::{create_task, create_task_with_args, init_repo, run_cli, update_task};

#[test]
fn tui_once_renders_tasks_view_tabs_table_and_spec_indicators() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let in_progress_id = create_task(repo.path(), "TUI in-progress task");
    let status_update = update_task(repo.path(), &in_progress_id, &["--status", "in_progress"]);
    assert_eq!(status_update.cli.code, 0);
    let spec_attach = run_cli(
        repo.path(),
        [
            "spec",
            "attach",
            &in_progress_id,
            "--text",
            "# Spec\n## Overview\nAttached for parity coverage.",
        ],
    );
    assert_eq!(
        spec_attach.code, 0,
        "spec attach failed\nstdout:\n{}\nstderr:\n{}",
        spec_attach.stdout, spec_attach.stderr
    );

    let open_id = create_task(repo.path(), "TUI open task");

    let result = run_cli(repo.path(), ["tui", "--once"]);
    assert_eq!(
        result.code, 0,
        "tui list frame failed\nstdout:\n{}\nstderr:\n{}",
        result.stdout, result.stderr
    );
    let stdout = normalize_tui_output(&result.stdout);

    assert_top_tabs_present(&stdout);
    assert_tasks_table_header_present(&stdout);
    assert!(
        contains_text_case_insensitive(&stdout, "tasks"),
        "expected tasks heading marker\nstdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains(&in_progress_id),
        "expected in-progress task id in tasks view\nstdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains(&open_id),
        "expected open task id in tasks view\nstdout:\n{}",
        stdout
    );
    assert_task_has_spec_state(&stdout, &in_progress_id, "attached");
    assert_task_has_spec_state(&stdout, &open_id, "missing");
}

#[test]
fn tui_once_renders_board_view_with_three_columns_and_spec_indicators_on_cards() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let open_id = create_task(repo.path(), "TUI open board task");
    let in_progress_id = create_task(repo.path(), "TUI in-progress board task");
    let closed_id = create_task(repo.path(), "TUI closed board task");
    let in_progress_update =
        update_task(repo.path(), &in_progress_id, &["--status", "in_progress"]);
    assert_eq!(in_progress_update.cli.code, 0);
    let closed_update = update_task(repo.path(), &closed_id, &["--status", "closed"]);
    assert_eq!(closed_update.cli.code, 0);
    let spec_attach = run_cli(
        repo.path(),
        [
            "spec",
            "attach",
            &in_progress_id,
            "--text",
            "# Spec\n## Overview\nBoard indicator parity coverage.",
        ],
    );
    assert_eq!(
        spec_attach.code, 0,
        "spec attach failed\nstdout:\n{}\nstderr:\n{}",
        spec_attach.stdout, spec_attach.stderr
    );

    let result = run_cli(
        repo.path(),
        [
            "tui",
            "--once",
            "--board",
            "--status",
            "open,in_progress,closed",
        ],
    );
    assert_eq!(
        result.code, 0,
        "tui board frame failed\nstdout:\n{}\nstderr:\n{}",
        result.stdout, result.stderr
    );
    let stdout = normalize_tui_output(&result.stdout);

    assert_top_tabs_present(&stdout);
    assert!(
        contains_text_case_insensitive(&stdout, "open"),
        "expected Open board column label\nstdout:\n{}",
        stdout
    );
    assert!(
        contains_text_case_insensitive(&stdout, "in progress"),
        "expected In Progress board column label\nstdout:\n{}",
        stdout
    );
    assert!(
        contains_text_case_insensitive(&stdout, "done"),
        "expected Done board column label\nstdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains(&open_id),
        "expected open task id in board output\nstdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains(&in_progress_id),
        "expected in-progress task id in board output\nstdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains(&closed_id),
        "expected closed task id in board output\nstdout:\n{}",
        stdout
    );
    assert_task_has_spec_state(&stdout, &in_progress_id, "attached");
    assert_task_has_spec_state(&stdout, &open_id, "missing");
}

#[test]
fn tui_once_epics_renders_progress_header_and_spec_states() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let epic_title = "TUI epics parity parent";
    let epic_id = create_task_with_args(repo.path(), epic_title, &["--kind", "epic"]);
    let child_with_spec_id = create_task_with_args(
        repo.path(),
        "TUI epics child with spec",
        &["--parent", epic_id.as_str()],
    );
    let child_missing_spec_id = create_task_with_args(
        repo.path(),
        "TUI epics child without spec",
        &["--parent", epic_id.as_str()],
    );

    let spec_attach = run_cli(
        repo.path(),
        [
            "spec",
            "attach",
            &child_with_spec_id,
            "--text",
            "# Spec\n## Overview\nEpics indicator parity coverage.",
        ],
    );
    assert_eq!(
        spec_attach.code, 0,
        "spec attach failed\nstdout:\n{}\nstderr:\n{}",
        spec_attach.stdout, spec_attach.stderr
    );

    let result = run_cli(repo.path(), ["tui", "--once", "--epics"]);
    assert_eq!(
        result.code, 0,
        "tui epics frame failed\nstdout:\n{}\nstderr:\n{}",
        result.stdout, result.stderr
    );

    let stdout = normalize_tui_output(&result.stdout);

    assert_top_tabs_present(&stdout);
    assert_tasks_table_header_present(&stdout);
    assert!(
        stdout.contains(&epic_id),
        "expected epic id in epics output\nstdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains(&child_with_spec_id),
        "expected child-with-spec id in epics output\nstdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains(&child_missing_spec_id),
        "expected child-without-spec id in epics output\nstdout:\n{}",
        stdout
    );
    assert!(
        stdout.lines().any(|line| {
            let line_norm = normalize_for_match(line);
            line_norm.contains("progress")
                && (line_norm.contains(&normalize_for_match(epic_title))
                    || line_norm.contains(&normalize_for_match(&epic_id)))
        }),
        "expected epic progress header in epics output\nstdout:\n{}",
        stdout
    );
    assert_task_has_spec_state(&stdout, &child_with_spec_id, "attached");
    assert_task_has_spec_state(&stdout, &child_missing_spec_id, "missing");
}

fn assert_task_has_spec_state(stdout: &str, task_id: &str, expected_state: &str) {
    let lines: Vec<&str> = stdout.lines().collect();
    let aliases = spec_state_aliases(expected_state);
    let mut matched = false;
    let mut context_lines = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        if !line.contains(task_id) {
            continue;
        }
        let window_end = (index + 2).min(lines.len().saturating_sub(1));
        let window = &lines[index..=window_end];
        context_lines.extend(window.iter().copied());
        let found = window.iter().any(|candidate| {
            let candidate_norm = normalize_for_match(candidate);
            aliases.iter().any(|alias| candidate_norm.contains(alias))
        });
        if found {
            matched = true;
            break;
        }
    }

    assert!(
        matched,
        "expected {task_id} to show spec {expected_state} indicator\nmatching lines:\n{}\nstdout:\n{}",
        context_lines.join("\n"),
        stdout
    );
}

fn assert_top_tabs_present(stdout: &str) {
    assert!(
        contains_line_with_tokens(stdout, &["tasks", "epics", "board"]),
        "expected tabs row with Tasks/Epics/Board\nstdout:\n{}",
        stdout
    );
}

fn assert_tasks_table_header_present(stdout: &str) {
    assert!(
        contains_line_with_tokens(
            stdout,
            &[
                "id", "type", "title", "status", "assignee", "priority", "spec"
            ],
        ),
        "expected table header with ID/Type/Title/Status/Assignee/Priority/Spec\nstdout:\n{}",
        stdout
    );
}

fn contains_line_with_tokens(stdout: &str, tokens: &[&str]) -> bool {
    stdout.lines().any(|line| {
        let line_norm = normalize_for_match(line);
        tokens.iter().all(|token| {
            let token_norm = normalize_for_match(token);
            line_norm.contains(&token_norm)
        })
    })
}

fn contains_text_case_insensitive(stdout: &str, needle: &str) -> bool {
    normalize_for_match(stdout).contains(&normalize_for_match(needle))
}

fn spec_state_aliases(expected_state: &str) -> &'static [&'static str] {
    match expected_state {
        "attached" => &["attached", "spec attached", "s✓"],
        "missing" => &["missing", "spec missing", "s!"],
        "invalid" => &["invalid", "invalid metadata", "spec invalid", "s✕", "s×"],
        _ => panic!("unsupported spec state '{expected_state}' in test helper"),
    }
}

fn normalize_tui_output(value: &str) -> String {
    strip_ansi(value).replace('\r', "")
}

fn normalize_for_match(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_ansi(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    loop {
                        let Some(control) = chars.next() else {
                            break;
                        };
                        if ('@'..='~').contains(&control) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    loop {
                        let Some(control) = chars.next() else {
                            break;
                        };
                        if control == '\u{7}' {
                            break;
                        }
                        if control == '\u{1b}' && matches!(chars.peek(), Some('\\')) {
                            chars.next();
                            break;
                        }
                    }
                }
                Some('P' | 'X' | '^' | '_') => {
                    chars.next();
                    loop {
                        let Some(control) = chars.next() else {
                            break;
                        };
                        if control == '\u{1b}' && matches!(chars.peek(), Some('\\')) {
                            chars.next();
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => break,
            }
            continue;
        }
        out.push(ch);
    }
    out
}
