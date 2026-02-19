use crate::app::service::TasqueService;
use crate::app::service_types::ListFilter;
use crate::cli::render::{
    TreeRenderOptions, format_meta_badge, format_status, format_status_text, render_task_tree,
    truncate_with_ellipsis,
};
use crate::cli::terminal::{Density, resolve_density, resolve_width};
use crate::errors::TsqError;
use crate::output::{err_envelope, ok_envelope};
use crate::types::{Task, TaskStatus, TaskTreeNode};
use chrono::Utc;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::IsTerminal;
use std::thread;
use std::time::Duration;

const ANSI_CLEAR: &str = "\x1b[2J\x1b[H";

#[derive(Debug, Clone)]
pub struct WatchOptions {
    pub interval: i64,
    pub statuses: Vec<TaskStatus>,
    pub assignee: Option<String>,
    pub tree: bool,
    pub once: bool,
    pub json: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchSummary {
    pub total: usize,
    pub open: usize,
    pub in_progress: usize,
    pub blocked: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchFrameFilters {
    pub status: Vec<TaskStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchFrameData {
    pub frame_ts: String,
    pub interval_s: i64,
    pub filters: WatchFrameFilters,
    pub summary: WatchSummary,
    pub tasks: Vec<Task>,
}

enum FrameResult {
    Ok(WatchFrameData),
    Err {
        error: String,
        code: String,
        exit_code: i32,
    },
}

pub fn start_watch(service: &TasqueService, options: WatchOptions) -> i32 {
    if let Err(error) = validate_options(&options) {
        output_frame(
            &FrameResult::Err {
                error: error.message,
                code: error.code,
                exit_code: error.exit_code,
            },
            &options,
            false,
            false,
        );
        return 1;
    }

    if options.once {
        let frame = load_frame(service, &options);
        output_frame(&frame, &options, false, false);
        return match frame {
            FrameResult::Ok(_) => 0,
            FrameResult::Err { exit_code, .. } => exit_code,
        };
    }

    let can_clear = std::io::stdout().is_terminal() && !options.json;
    let can_interact =
        std::io::stdout().is_terminal() && std::io::stdin().is_terminal() && !options.json;
    let mut paused = false;
    let mut last_good_frame: Option<WatchFrameData> = None;
    let interval = Duration::from_secs(options.interval as u64);

    let _raw_mode = if can_interact {
        match RawModeGuard::enable() {
            Ok(guard) => Some(guard),
            Err(error) => {
                output_watch_error(
                    &options,
                    format!("failed enabling interactive controls: {}", error),
                    "WATCH_INTERACTIVE_ERROR",
                    paused,
                );
                None
            }
        }
    } else {
        None
    };
    let interactive = can_interact && _raw_mode.is_some();

    refresh_frame(service, &options, can_clear, paused, &mut last_good_frame);

    if interactive {
        loop {
            match event::poll(interval) {
                Ok(true) => match event::read() {
                    Ok(Event::Key(key)) => {
                        if should_quit_on_key(&key) {
                            break;
                        }
                        if is_refresh_key(&key) {
                            refresh_frame(
                                service,
                                &options,
                                can_clear,
                                paused,
                                &mut last_good_frame,
                            );
                            continue;
                        }
                        if is_pause_toggle_key(&key) {
                            paused = !paused;
                            if let Some(frame) = last_good_frame.clone() {
                                output_frame(&FrameResult::Ok(frame), &options, can_clear, paused);
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        output_watch_error(
                            &options,
                            format!("interactive input failed: {}", error),
                            "WATCH_INTERACTIVE_ERROR",
                            paused,
                        );
                    }
                },
                Ok(false) => {
                    if !paused {
                        refresh_frame(service, &options, can_clear, paused, &mut last_good_frame);
                    }
                }
                Err(error) => {
                    output_watch_error(
                        &options,
                        format!("interactive poll failed: {}", error),
                        "WATCH_INTERACTIVE_ERROR",
                        paused,
                    );
                    thread::sleep(interval);
                }
            }
        }
        return 0;
    }

    loop {
        thread::sleep(interval);
        refresh_frame(service, &options, can_clear, paused, &mut last_good_frame);
    }
}

fn refresh_frame(
    service: &TasqueService,
    options: &WatchOptions,
    clear_screen: bool,
    paused: bool,
    last_good_frame: &mut Option<WatchFrameData>,
) {
    match load_frame(service, options) {
        FrameResult::Ok(data) => {
            *last_good_frame = Some(data.clone());
            output_frame(&FrameResult::Ok(data), options, clear_screen, paused);
        }
        FrameResult::Err {
            error,
            code,
            exit_code,
        } => {
            if !options.json
                && let Some(previous) = last_good_frame.clone()
            {
                output_frame(&FrameResult::Ok(previous), options, clear_screen, paused);
            }
            output_frame(
                &FrameResult::Err {
                    error,
                    code,
                    exit_code,
                },
                options,
                false,
                paused,
            );
        }
    }
}

fn output_watch_error(options: &WatchOptions, error: String, code: &str, paused: bool) {
    output_frame(
        &FrameResult::Err {
            error,
            code: code.to_string(),
            exit_code: 2,
        },
        options,
        false,
        paused,
    );
}

fn should_quit_on_key(key: &KeyEvent) -> bool {
    match key.code {
        KeyCode::Char(value) => {
            value.eq_ignore_ascii_case(&'q')
                || (value.eq_ignore_ascii_case(&'c')
                    && key.modifiers.contains(KeyModifiers::CONTROL))
        }
        _ => false,
    }
}

fn is_refresh_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char(value) if value.eq_ignore_ascii_case(&'r'))
}

fn is_pause_toggle_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char(value) if value.eq_ignore_ascii_case(&'p'))
}

struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> std::io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

fn load_frame(service: &TasqueService, options: &WatchOptions) -> FrameResult {
    let filter = ListFilter {
        statuses: Some(options.statuses.clone()),
        assignee: options.assignee.clone(),
        external_ref: None,
        discovered_from: None,
        kind: None,
        label: None,
        label_any: None,
        created_after: None,
        updated_after: None,
        closed_after: None,
        unassigned: false,
        ids: None,
        planning_state: None,
        dep_type: None,
        dep_direction: None,
    };

    match service.list(&filter) {
        Ok(tasks) => {
            let sorted = sort_watch_tasks(tasks);
            let summary = compute_summary(&sorted);
            FrameResult::Ok(WatchFrameData {
                frame_ts: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                interval_s: options.interval,
                filters: WatchFrameFilters {
                    status: options.statuses.clone(),
                    assignee: options.assignee.clone(),
                },
                summary,
                tasks: sorted,
            })
        }
        Err(error) => FrameResult::Err {
            error: error.message,
            code: error.code,
            exit_code: error.exit_code,
        },
    }
}

fn output_frame(frame: &FrameResult, options: &WatchOptions, clear_screen: bool, paused: bool) {
    if options.json {
        output_json_frame(frame);
        return;
    }
    output_human_frame(frame, options, clear_screen, paused);
}

fn output_json_frame(frame: &FrameResult) {
    match frame {
        FrameResult::Ok(data) => {
            let envelope = ok_envelope("tsq watch", data);
            println!(
                "{}",
                serde_json::to_string(&envelope).unwrap_or_else(|_| "{}".to_string())
            );
        }
        FrameResult::Err { error, code, .. } => {
            let envelope = err_envelope(
                "tsq watch",
                code.to_string(),
                error.to_string(),
                Option::<serde_json::Value>::None,
            );
            println!(
                "{}",
                serde_json::to_string(&envelope).unwrap_or_else(|_| "{}".to_string())
            );
        }
    }
}

fn output_human_frame(
    frame: &FrameResult,
    options: &WatchOptions,
    clear_screen: bool,
    paused: bool,
) {
    let width = resolve_width(None);
    if clear_screen {
        print!("{}", ANSI_CLEAR);
    }

    match frame {
        FrameResult::Err { error, .. } => {
            println!("refresh failed: {}", error);
        }
        FrameResult::Ok(data) => {
            let mut lines = Vec::new();
            lines.push(render_header(data, paused, width));
            lines.push(render_summary(&data.summary));
            lines.push("─".repeat(width));
            if data.tasks.is_empty() {
                lines.push("no active tasks".to_string());
            } else if options.tree {
                let tree_nodes = build_watch_tree(&data.tasks);
                let tree_lines =
                    render_task_tree(&tree_nodes, TreeRenderOptions { width: Some(width) });
                lines.extend(
                    tree_lines
                        .into_iter()
                        .filter(|line| !line.starts_with("total=")),
                );
            } else {
                lines.extend(render_flat_tasks(&data.tasks, width));
            }
            lines.push("─".repeat(width));
            if std::io::stdout().is_terminal() {
                lines.push(if paused {
                    "q quit  r refresh  p resume".to_string()
                } else {
                    "q quit  r refresh  p pause".to_string()
                });
            }
            for line in lines {
                println!("{}", line);
            }
        }
    }
}

fn render_header(data: &WatchFrameData, paused: bool, width: usize) -> String {
    let filter_str = format!(
        "status:{}{}",
        data.filters
            .status
            .iter()
            .map(|status| status_to_string(*status))
            .collect::<Vec<_>>()
            .join(","),
        data.filters
            .assignee
            .as_ref()
            .map(|value| format!(" assignee:{}", value))
            .unwrap_or_default()
    );
    let pause_tag = if paused { " ⏸ paused" } else { "" };
    if resolve_density(width) == Density::Narrow {
        let short_ts = if data.frame_ts.len() >= 19 {
            format!("{}Z", &data.frame_ts[11..19])
        } else {
            data.frame_ts.clone()
        };
        return format!(
            "[tsq watch] refreshed={} interval={}s{}",
            short_ts, data.interval_s, pause_tag
        );
    }

    format!(
        "[tsq watch]  refreshed={}  interval={}s  filter={}{}",
        data.frame_ts, data.interval_s, filter_str, pause_tag
    )
}

fn render_summary(summary: &WatchSummary) -> String {
    format!(
        "active={}  in_progress={}  open={}  blocked={}",
        summary.total, summary.in_progress, summary.open, summary.blocked
    )
}

fn render_flat_tasks(tasks: &[Task], width: usize) -> Vec<String> {
    let density = resolve_density(width);
    let mut lines = Vec::new();
    for task in tasks {
        let status = format_status(task.status);
        let status_text = format_status_text(task.status);
        let meta = format_meta_badge(task);
        if density == Density::Narrow {
            let title_width =
                (width as isize - status_text.len() as isize - 1 - task.id.len() as isize - 1)
                    .max(12) as usize;
            lines.push(format!(
                "{} {} {}",
                status,
                task.id,
                truncate_with_ellipsis(&task.title, title_width)
            ));
            lines.push(format!("  {}", meta));
        } else {
            lines.push(format!("{}  {}  {}  {}", status, task.id, task.title, meta));
        }
    }
    lines
}

fn validate_options(options: &WatchOptions) -> Result<(), TsqError> {
    if options.interval < 1 || options.interval > 60 {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "interval must be between 1 and 60 seconds",
            1,
        ));
    }
    Ok(())
}

fn sort_watch_tasks(mut tasks: Vec<Task>) -> Vec<Task> {
    tasks.sort_by(|a, b| {
        let sa = status_order(a.status);
        let sb = status_order(b.status);
        if sa != sb {
            return sa.cmp(&sb);
        }
        if a.priority != b.priority {
            return a.priority.cmp(&b.priority);
        }
        if a.created_at != b.created_at {
            return a.created_at.cmp(&b.created_at);
        }
        a.id.cmp(&b.id)
    });
    tasks
}

fn status_order(status: TaskStatus) -> usize {
    match status {
        TaskStatus::InProgress => 0,
        TaskStatus::Open => 1,
        TaskStatus::Blocked => 2,
        TaskStatus::Deferred => 3,
        TaskStatus::Closed => 4,
        TaskStatus::Canceled => 5,
    }
}

fn compute_summary(tasks: &[Task]) -> WatchSummary {
    let mut summary = WatchSummary {
        total: tasks.len(),
        open: 0,
        in_progress: 0,
        blocked: 0,
    };
    for task in tasks {
        match task.status {
            TaskStatus::Open => summary.open += 1,
            TaskStatus::InProgress => summary.in_progress += 1,
            TaskStatus::Blocked => summary.blocked += 1,
            _ => {}
        }
    }
    summary
}

fn build_watch_tree(tasks: &[Task]) -> Vec<TaskTreeNode> {
    let by_id: HashMap<String, Task> = tasks
        .iter()
        .cloned()
        .map(|task| (task.id.clone(), task))
        .collect();
    let mut children_by_parent: HashMap<String, Vec<Task>> = HashMap::new();
    let mut roots = Vec::new();

    for task in tasks {
        if let Some(parent) = task.parent_id.as_ref()
            && by_id.contains_key(parent)
        {
            children_by_parent
                .entry(parent.clone())
                .or_default()
                .push(task.clone());
            continue;
        }
        roots.push(task.clone());
    }

    fn build_node(task: &Task, children_by_parent: &HashMap<String, Vec<Task>>) -> TaskTreeNode {
        let children = children_by_parent
            .get(&task.id)
            .map(|values| {
                values
                    .iter()
                    .map(|child| build_node(child, children_by_parent))
                    .collect()
            })
            .unwrap_or_default();
        TaskTreeNode {
            task: task.clone(),
            blockers: Vec::new(),
            dependents: Vec::new(),
            blocker_edges: None,
            dependent_edges: None,
            children,
        }
    }

    roots
        .iter()
        .map(|task| build_node(task, &children_by_parent))
        .collect()
}

fn status_to_string(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Open => "open",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Closed => "closed",
        TaskStatus::Canceled => "canceled",
        TaskStatus::Deferred => "deferred",
    }
}
