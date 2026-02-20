mod common;

use common::{create_task, init_repo, run_cli, update_task};

#[test]
fn tui_once_renders_list_view_with_selected_inspector_metadata() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let in_progress_id = create_task(repo.path(), "TUI in-progress task");
    let status_update = update_task(repo.path(), &in_progress_id, &["--status", "in_progress"]);
    assert_eq!(status_update.cli.code, 0);

    create_task(repo.path(), "TUI open task");

    let result = run_cli(repo.path(), ["tui", "--once"]);
    assert_eq!(
        result.code, 0,
        "tui list frame failed\nstdout:\n{}\nstderr:\n{}",
        result.stdout, result.stderr
    );

    assert!(
        result.stdout.contains("[tsq tui]"),
        "expected tui header in output\nstdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("view=list"),
        "expected list view marker\nstdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("list view"),
        "expected list view section marker\nstdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("inspector"),
        "expected inspector section marker\nstdout:\n{}",
        result.stdout
    );
    assert!(
        result
            .stdout
            .contains(&format!("selected={}", in_progress_id)),
        "expected selected id marker for inspector parity\nstdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("status=in_progress"),
        "expected inspector status metadata\nstdout:\n{}",
        result.stdout
    );
}

#[test]
fn tui_once_renders_board_view_with_kanban_status_buckets() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let open_id = create_task(repo.path(), "TUI open board task");
    let blocked_id = create_task(repo.path(), "TUI blocked board task");
    let status_update = update_task(repo.path(), &blocked_id, &["--status", "blocked"]);
    assert_eq!(status_update.cli.code, 0);

    let result = run_cli(
        repo.path(),
        [
            "tui",
            "--once",
            "--board",
            "--status",
            "open,in_progress,blocked",
        ],
    );
    assert_eq!(
        result.code, 0,
        "tui board frame failed\nstdout:\n{}\nstderr:\n{}",
        result.stdout, result.stderr
    );

    assert!(
        result.stdout.contains("view=board"),
        "expected board view marker\nstdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("board view (kanban)"),
        "expected board heading\nstdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("OPEN ("),
        "expected OPEN bucket marker\nstdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("BLOCKED ("),
        "expected BLOCKED bucket marker\nstdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains(&open_id),
        "expected open task id in board output\nstdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains(&blocked_id),
        "expected blocked task id in board output\nstdout:\n{}",
        result.stdout
    );
}
