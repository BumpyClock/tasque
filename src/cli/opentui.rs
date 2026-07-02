use crate::app::runtime::find_tasque_root;
use crate::cli::tui::{TuiOptions, TuiView};
use crate::cli::watch::WatchOptions;
use crate::types::TaskStatus;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DEFAULT_RUNTIME: &str = "bun";
#[cfg(windows)]
const BUNDLED_TUI_BINARY: &str = "tsq-tui.exe";
#[cfg(not(windows))]
const BUNDLED_TUI_BINARY: &str = "tsq-tui";

pub fn should_launch_opentui(options: &TuiOptions) -> bool {
    interactive_launch_ready(options.json, options.once) && launch_target_available()
}

pub fn should_launch_opentui_watch(options: &WatchOptions) -> bool {
    interactive_launch_ready(options.json, options.once) && launch_target_available()
}

fn interactive_launch_ready(json: bool, once: bool) -> bool {
    if std::env::var("TSQ_OPENTUI_DISABLE")
        .ok()
        .as_deref()
        .is_some_and(|value| value == "1")
    {
        return false;
    }
    !json
        && !once
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
}

fn launch_target_available() -> bool {
    if explicit_bundled_tui_path().is_some() && resolve_bundled_tui_path().is_some() {
        return true;
    }

    if explicit_entry_path().is_some() {
        return true;
    }

    if resolve_bundled_tui_path().is_some() {
        return true;
    }

    let Some(entry) = resolve_entry_path() else {
        return false;
    };
    runtime_is_available(&resolve_runtime()) && dependencies_are_available(&entry)
}

pub fn launch_opentui(options: &TuiOptions) -> Result<i32, String> {
    let mut command = build_launch_command()?;
    apply_shared_env(&mut command);
    // Clear any inherited watch mode so `tsq tui` always renders the tabbed App.
    command.env_remove("TSQ_TUI_MODE");
    command.env("TSQ_TUI_INTERVAL", options.interval.to_string());
    command.env("TSQ_TUI_STATUS", status_csv(&options.statuses));
    command.env("TSQ_TUI_VIEW", view_to_env(options.view));

    if let Some(assignee) = options.assignee.as_deref() {
        command.env("TSQ_TUI_ASSIGNEE", assignee);
    }

    run_command(command)
}

pub fn launch_opentui_watch(options: &WatchOptions) -> Result<i32, String> {
    let mut command = build_launch_command()?;
    apply_shared_env(&mut command);
    command.env("TSQ_TUI_MODE", "watch");
    command.env("TSQ_TUI_INTERVAL", options.interval.to_string());
    command.env("TSQ_TUI_STATUS", status_csv(&options.statuses));
    command.env("TSQ_WATCH_TREE", if options.tree { "1" } else { "0" });

    if let Some(assignee) = options.assignee.as_deref() {
        command.env("TSQ_TUI_ASSIGNEE", assignee);
    }

    run_command(command)
}

fn build_launch_command() -> Result<Command, String> {
    let prefer_bundle = explicit_bundled_tui_path().is_some() || explicit_entry_path().is_none();
    let command = if let Some(bundle) = if prefer_bundle {
        resolve_bundled_tui_path()
    } else {
        None
    } {
        Command::new(bundle)
    } else {
        let entry = resolve_entry_path().ok_or_else(|| {
            "OpenTUI entrypoint not found (expected tui-opentui/src/index.tsx)".to_string()
        })?;
        let runtime = resolve_runtime();
        let mut command = Command::new(&runtime);
        command.arg("run").arg(&entry);
        command
    };
    Ok(command)
}

fn apply_shared_env(command: &mut Command) {
    if let Ok(bin) = std::env::current_exe() {
        command.env("TSQ_TUI_BIN", bin);
    }
    if let Some(root) = find_tasque_root() {
        command.current_dir(root);
    }
}

fn run_command(mut command: Command) -> Result<i32, String> {
    let status = command
        .status()
        .map_err(|error| format!("failed launching OpenTUI: {error}"))?;
    Ok(status.code().unwrap_or(1))
}

fn resolve_bundled_tui_path() -> Option<PathBuf> {
    if let Some(value) = explicit_bundled_tui_path() {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
    }

    let current_exe = std::env::current_exe().ok()?;
    let candidate = current_exe.parent()?.join(BUNDLED_TUI_BINARY);
    candidate.is_file().then_some(candidate)
}

fn explicit_bundled_tui_path() -> Option<String> {
    std::env::var("TSQ_OPENTUI_BIN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn resolve_entry_path() -> Option<PathBuf> {
    if let Some(value) = explicit_entry_path() {
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

fn explicit_entry_path() -> Option<String> {
    std::env::var("TSQ_OPENTUI_ENTRY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn resolve_runtime() -> String {
    std::env::var("TSQ_OPENTUI_RUNTIME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_RUNTIME.to_string())
}

fn runtime_is_available(runtime: &str) -> bool {
    Command::new(runtime)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn dependencies_are_available(entry: &Path) -> bool {
    let Some(project_dir) = entry.parent().and_then(Path::parent) else {
        return false;
    };
    ["@opentui/core", "@opentui/react", "react"]
        .iter()
        .all(|name| dependency_path(project_dir, name).exists())
}

fn dependency_path(project_dir: &Path, name: &str) -> PathBuf {
    let mut path = project_dir.join("node_modules");
    for segment in name.split('/') {
        path.push(segment);
    }
    path
}

fn view_to_env(view: TuiView) -> &'static str {
    match view {
        TuiView::List => "tasks",
        TuiView::Epics => "epics",
        TuiView::Board => "board",
    }
}

fn status_csv(statuses: &[TaskStatus]) -> String {
    statuses
        .iter()
        .map(|status| crate::domain::event_payload_codecs::task_status_as_str(*status))
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

    #[test]
    fn detects_opentui_dependencies_next_to_entry() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project = temp.path().join("tui-opentui");
        std::fs::create_dir_all(project.join("src")).expect("src");
        for dep in ["@opentui/core", "@opentui/react", "react"] {
            std::fs::create_dir_all(dependency_path(&project, dep)).expect("dep");
        }
        let entry = project.join("src").join("index.tsx");
        std::fs::write(&entry, "").expect("entry");

        assert!(dependencies_are_available(&entry));
    }

    #[test]
    fn rejects_entry_without_opentui_dependencies() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project = temp.path().join("tui-opentui");
        std::fs::create_dir_all(project.join("src")).expect("src");
        let entry = project.join("src").join("index.tsx");
        std::fs::write(&entry, "").expect("entry");

        assert!(!dependencies_are_available(&entry));
    }

    #[test]
    fn resolves_explicit_bundled_tui_binary() {
        let temp = tempfile::tempdir().expect("tempdir");
        let binary = temp.path().join(BUNDLED_TUI_BINARY);
        std::fs::write(&binary, "").expect("binary");

        unsafe {
            std::env::set_var("TSQ_OPENTUI_BIN", &binary);
        }
        let resolved = resolve_bundled_tui_path();
        unsafe {
            std::env::remove_var("TSQ_OPENTUI_BIN");
        }

        assert_eq!(resolved.as_deref(), Some(binary.as_path()));
    }
}
