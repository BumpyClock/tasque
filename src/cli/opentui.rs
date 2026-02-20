use crate::app::runtime::find_tasque_root;
use crate::cli::tui::{TuiOptions, TuiView};
use crate::types::TaskStatus;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_RUNTIME: &str = "bun";

pub fn should_launch_opentui(options: &TuiOptions) -> bool {
    if std::env::var("TSQ_OPENTUI_DISABLE")
        .ok()
        .as_deref()
        .is_some_and(|value| value == "1")
    {
        return false;
    }
    !options.json
        && !options.once
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
}

pub fn launch_opentui(options: &TuiOptions) -> Result<i32, String> {
    let entry = resolve_entry_path().ok_or_else(|| {
        "OpenTUI entrypoint not found (expected tui-opentui/src/index.tsx)".to_string()
    })?;

    let runtime = std::env::var("TSQ_OPENTUI_RUNTIME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_RUNTIME.to_string());

    let mut command = Command::new(&runtime);
    command.arg("run").arg(&entry);

    if let Ok(bin) = std::env::current_exe() {
        command.env("TSQ_TUI_BIN", bin);
    }

    command.env("TSQ_TUI_INTERVAL", options.interval.to_string());
    command.env("TSQ_TUI_STATUS", status_csv(options));
    command.env("TSQ_TUI_VIEW", view_to_env(options.view));

    if let Some(assignee) = options.assignee.as_deref() {
        command.env("TSQ_TUI_ASSIGNEE", assignee);
    }

    if let Some(root) = find_tasque_root() {
        command.current_dir(root);
    }

    let status = command
        .status()
        .map_err(|error| format!("failed launching OpenTUI with `{runtime}`: {error}"))?;
    Ok(status.code().unwrap_or(1))
}

fn resolve_entry_path() -> Option<PathBuf> {
    if let Some(value) = std::env::var("TSQ_OPENTUI_ENTRY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
    }

    let mut candidates = Vec::new();

    if let Ok(current) = std::env::current_dir() {
        candidates.push(current.join("tui-opentui").join("src").join("index.tsx"));
    }
    if let Some(root) = find_tasque_root() {
        candidates.push(root.join("tui-opentui").join("src").join("index.tsx"));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tui-opentui")
            .join("src")
            .join("index.tsx"),
    );

    candidates.into_iter().find(|path| path.is_file())
}

fn view_to_env(view: TuiView) -> &'static str {
    match view {
        TuiView::List => "tasks",
        TuiView::Epics => "epics",
        TuiView::Board => "board",
    }
}

fn status_csv(options: &TuiOptions) -> String {
    options
        .statuses
        .iter()
        .map(|status| match status {
            TaskStatus::Open => "open",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Deferred => "deferred",
            TaskStatus::Closed => "closed",
            TaskStatus::Canceled => "canceled",
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_mapping_is_stable() {
        assert_eq!(view_to_env(TuiView::List), "tasks");
        assert_eq!(view_to_env(TuiView::Epics), "epics");
        assert_eq!(view_to_env(TuiView::Board), "board");
    }
}
