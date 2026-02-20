use crate::app::service::TasqueService;
use crate::app::service_types::ListFilter;
use crate::cli::render::{
    format_meta_badge, format_status, format_status_text, truncate_with_ellipsis,
};
use crate::cli::style;
use crate::cli::terminal::{Density, resolve_density, resolve_width};
use crate::errors::TsqError;
use crate::output::{err_envelope, ok_envelope};
use crate::types::{Task, TaskStatus};
use chrono::Utc;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use serde::{Deserialize, Serialize};
use std::io::IsTerminal;
use std::thread;
use std::time::Duration;

const ANSI_CLEAR: &str = "\x1b[2J\x1b[H";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TuiView {
    List,
    Board,
}

#[derive(Debug, Clone)]
pub struct TuiOptions {
    pub interval: i64,
    pub statuses: Vec<TaskStatus>,
    pub assignee: Option<String>,
    pub once: bool,
    pub json: bool,
    pub view: TuiView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiSummary {
    pub total: usize,
    pub open: usize,
    pub in_progress: usize,
    pub blocked: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiFrameFilters {
    pub status: Vec<TaskStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiFrameData {
    pub frame_ts: String,
    pub interval_s: i64,
    pub view: TuiView,
    pub filters: TuiFrameFilters,
    pub summary: TuiSummary,
    pub tasks: Vec<Task>,
    pub selected_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_task_id: Option<String>,
}

enum FrameResult {
    Ok(TuiFrameData),
    Err {
        error: String,
        code: String,
        exit_code: i32,
    },
}

pub fn start_tui(service: &TasqueService, options: TuiOptions) -> i32 {
    if let Err(error) = validate_options(&options) {
        output_frame(
            &FrameResult::Err {
                error: error.message,
                code: error.code,
                exit_code: error.exit_code,
            },
            options.json,
            false,
            false,
        );
        return 1;
    }

    let can_clear = std::io::stdout().is_terminal() && !options.json;
    let can_interact =
        std::io::stdout().is_terminal() && std::io::stdin().is_terminal() && !options.json;
    let mut view = options.view;
    let mut paused = false;
    let mut selected_index = 0usize;
    let interval = Duration::from_secs(options.interval as u64);
    let mut last_good_frame: Option<TuiFrameData> = None;

    if options.once {
        let frame = load_frame(service, &options, view, selected_index);
        output_frame(&frame, options.json, false, false);
        return match frame {
            FrameResult::Ok(_) => 0,
            FrameResult::Err { exit_code, .. } => exit_code,
        };
    }

    let raw_mode = if can_interact {
        match RawModeGuard::enable() {
            Ok(guard) => Some(guard),
            Err(error) => {
                output_tui_error(
                    options.json,
                    format!("failed enabling interactive controls: {}", error),
                    "TUI_INTERACTIVE_ERROR",
                    paused,
                );
                None
            }
        }
    } else {
        None
    };
    let interactive = can_interact && raw_mode.is_some();

    refresh_frame(
        service,
        &options,
        view,
        &mut selected_index,
        can_clear,
        paused,
        &mut last_good_frame,
    );

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
                                view,
                                &mut selected_index,
                                can_clear,
                                paused,
                                &mut last_good_frame,
                            );
                            continue;
                        }
                        if is_pause_toggle_key(&key) {
                            paused = !paused;
                            if let Some(frame) = last_good_frame.clone() {
                                output_frame(
                                    &FrameResult::Ok(frame),
                                    options.json,
                                    can_clear,
                                    paused,
                                );
                            }
                            continue;
                        }
                        if is_switch_view_key(&key) {
                            view = toggle_view(view);
                            refresh_frame(
                                service,
                                &options,
                                view,
                                &mut selected_index,
                                can_clear,
                                paused,
                                &mut last_good_frame,
                            );
                            continue;
                        }
                        if (is_select_up_key(&key) || is_select_down_key(&key))
                            && let Some(frame) = last_good_frame.as_mut()
                            && !frame.tasks.is_empty()
                        {
                            if is_select_up_key(&key) {
                                selected_index = selected_index.saturating_sub(1);
                            } else {
                                selected_index = (selected_index + 1).min(frame.tasks.len() - 1);
                            }
                            apply_selection(frame, view, selected_index);
                            output_frame(
                                &FrameResult::Ok(frame.clone()),
                                options.json,
                                can_clear,
                                paused,
                            );
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        output_tui_error(
                            options.json,
                            format!("interactive input failed: {}", error),
                            "TUI_INTERACTIVE_ERROR",
                            paused,
                        );
                    }
                },
                Ok(false) => {
                    if !paused {
                        refresh_frame(
                            service,
                            &options,
                            view,
                            &mut selected_index,
                            can_clear,
                            paused,
                            &mut last_good_frame,
                        );
                    }
                }
                Err(error) => {
                    output_tui_error(
                        options.json,
                        format!("interactive poll failed: {}", error),
                        "TUI_INTERACTIVE_ERROR",
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
        refresh_frame(
            service,
            &options,
            view,
            &mut selected_index,
            can_clear,
            paused,
            &mut last_good_frame,
        );
    }
}

fn refresh_frame(
    service: &TasqueService,
    options: &TuiOptions,
    view: TuiView,
    selected_index: &mut usize,
    clear_screen: bool,
    paused: bool,
    last_good_frame: &mut Option<TuiFrameData>,
) {
    match load_frame(service, options, view, *selected_index) {
        FrameResult::Ok(data) => {
            if let Some(index) = data.selected_index {
                *selected_index = index;
            } else {
                *selected_index = 0;
            }
            *last_good_frame = Some(data.clone());
            output_frame(&FrameResult::Ok(data), options.json, clear_screen, paused);
        }
        FrameResult::Err {
            error,
            code,
            exit_code,
        } => {
            if !options.json
                && let Some(previous) = last_good_frame.clone()
            {
                output_frame(
                    &FrameResult::Ok(previous),
                    options.json,
                    clear_screen,
                    paused,
                );
            }
            output_frame(
                &FrameResult::Err {
                    error,
                    code,
                    exit_code,
                },
                options.json,
                false,
                paused,
            );
        }
    }
}

fn output_tui_error(json: bool, error: String, code: &str, paused: bool) {
    output_frame(
        &FrameResult::Err {
            error,
            code: code.to_string(),
            exit_code: 2,
        },
        json,
        false,
        paused,
    );
}

fn should_quit_on_key(key: &KeyEvent) -> bool {
    if !is_press_like(key) {
        return false;
    }
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
    if !is_press_like(key) {
        return false;
    }
    matches!(key.code, KeyCode::Char(value) if value.eq_ignore_ascii_case(&'r'))
}

fn is_pause_toggle_key(key: &KeyEvent) -> bool {
    if !is_press_like(key) {
        return false;
    }
    matches!(key.code, KeyCode::Char(value) if value.eq_ignore_ascii_case(&'p'))
}

fn is_switch_view_key(key: &KeyEvent) -> bool {
    if !is_press_like(key) {
        return false;
    }
    matches!(key.code, KeyCode::Tab)
}

fn is_select_up_key(key: &KeyEvent) -> bool {
    if !is_press_like(key) {
        return false;
    }
    matches!(key.code, KeyCode::Up)
}

fn is_select_down_key(key: &KeyEvent) -> bool {
    if !is_press_like(key) {
        return false;
    }
    matches!(key.code, KeyCode::Down)
}

fn is_press_like(key: &KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

fn toggle_view(view: TuiView) -> TuiView {
    match view {
        TuiView::List => TuiView::Board,
        TuiView::Board => TuiView::List,
    }
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

fn load_frame(
    service: &TasqueService,
    options: &TuiOptions,
    view: TuiView,
    selected_index: usize,
) -> FrameResult {
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
            let sorted = sort_tui_tasks(tasks);
            let summary = compute_summary(&sorted);
            let selected = if sorted.is_empty() {
                None
            } else {
                Some(selected_index.min(sorted.len() - 1))
            };
            let selected_task_id = selected.map(|index| sorted[index].id.clone());
            FrameResult::Ok(TuiFrameData {
                frame_ts: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                interval_s: options.interval,
                view,
                filters: TuiFrameFilters {
                    status: options.statuses.clone(),
                    assignee: options.assignee.clone(),
                },
                summary,
                tasks: sorted,
                selected_index: selected,
                selected_task_id,
            })
        }
        Err(error) => FrameResult::Err {
            error: error.message,
            code: error.code,
            exit_code: error.exit_code,
        },
    }
}

fn output_frame(frame: &FrameResult, json: bool, clear_screen: bool, paused: bool) {
    if json {
        output_json_frame(frame);
        return;
    }
    output_human_frame(frame, clear_screen, paused);
}

fn output_json_frame(frame: &FrameResult) {
    match frame {
        FrameResult::Ok(data) => {
            let envelope = ok_envelope("tsq tui", data);
            println!(
                "{}",
                serde_json::to_string(&envelope).unwrap_or_else(|_| "{}".to_string())
            );
        }
        FrameResult::Err { error, code, .. } => {
            let envelope = err_envelope(
                "tsq tui",
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

fn output_human_frame(frame: &FrameResult, clear_screen: bool, paused: bool) {
    let width = resolve_width(None);
    if clear_screen {
        print!("{}", ANSI_CLEAR);
    }

    match frame {
        FrameResult::Err { error, .. } => {
            println!("{}: {}", style::error("refresh failed"), error);
        }
        FrameResult::Ok(data) => {
            let mut lines = Vec::new();
            lines.push(render_header(data, paused, width));
            lines.push(render_summary(data));
            lines.push(style::muted(&"-".repeat(width)));
            if data.tasks.is_empty() {
                lines.push(style::muted("no active tasks"));
            } else {
                match data.view {
                    TuiView::List => lines.extend(render_list_view(data, width)),
                    TuiView::Board => lines.extend(render_board_view(data, width)),
                }
            }
            lines.push(style::muted(&"-".repeat(width)));
            lines.extend(render_inspector(data, width));
            lines.push(style::muted(&"-".repeat(width)));
            if std::io::stdout().is_terminal() {
                lines.push(if paused {
                    style::muted("q quit  Tab switch  r refresh  p resume  Up/Down select")
                } else {
                    style::muted("q quit  Tab switch  r refresh  p pause  Up/Down select")
                });
            }
            for line in lines {
                println!("{}", line);
            }
        }
    }
}

fn render_header(data: &TuiFrameData, paused: bool, width: usize) -> String {
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
    let pause_tag = if paused { " paused" } else { "" };
    let view = view_to_string(data.view);
    if resolve_density(width) == Density::Narrow {
        let short_ts = if data.frame_ts.len() >= 19 {
            format!("{}Z", &data.frame_ts[11..19])
        } else {
            data.frame_ts.clone()
        };
        return format!(
            "[tsq tui] view={} refreshed={} interval={}s{}",
            view, short_ts, data.interval_s, pause_tag
        );
    }
    style::heading(&format!(
        "[tsq tui] view={} refreshed={} interval={}s filter={}{}",
        view, data.frame_ts, data.interval_s, filter_str, pause_tag
    ))
}

fn render_summary(data: &TuiFrameData) -> String {
    let selected = data.selected_task_id.as_deref().unwrap_or("none");
    format!(
        "active={} in_progress={} open={} blocked={} selected={}",
        data.summary.total,
        data.summary.in_progress,
        data.summary.open,
        data.summary.blocked,
        selected
    )
}

fn render_list_view(data: &TuiFrameData, width: usize) -> Vec<String> {
    let density = resolve_density(width);
    let mut lines = vec![style::heading("list view")];
    for (index, task) in data.tasks.iter().enumerate() {
        let status = format_status(task.status);
        let status_text = format_status_text(task.status);
        let meta = format_meta_badge(task);
        let selected_marker = if data.selected_index == Some(index) {
            ">"
        } else {
            " "
        };
        if density == Density::Narrow {
            let title_width =
                (width as isize - status_text.len() as isize - 1 - task.id.len() as isize - 6)
                    .max(12) as usize;
            lines.push(format!(
                "{} {} {} {}",
                selected_marker,
                status,
                style::task_id(&task.id),
                truncate_with_ellipsis(&task.title, title_width)
            ));
            lines.push(format!("  {}", meta));
        } else {
            let title_width = (width as isize
                - status_text.len() as isize
                - task.id.len() as isize
                - meta.len() as isize
                - 10)
                .max(16) as usize;
            lines.push(format!(
                "{} {} {} {} {}",
                selected_marker,
                status,
                style::task_id(&task.id),
                truncate_with_ellipsis(&task.title, title_width),
                meta
            ));
        }
    }
    lines
}

fn render_board_view(data: &TuiFrameData, width: usize) -> Vec<String> {
    let density = resolve_density(width);
    let mut lines = vec![style::heading("board view (kanban)")];
    for status in board_status_order() {
        let indices: Vec<usize> = data
            .tasks
            .iter()
            .enumerate()
            .filter_map(|(index, task)| (task.status == status).then_some(index))
            .collect();
        if indices.is_empty() {
            continue;
        }
        lines.push(style::key(&format!(
            "{} ({})",
            board_bucket_label(status),
            indices.len()
        )));
        for index in indices {
            let task = &data.tasks[index];
            let selected_marker = if data.selected_index == Some(index) {
                ">"
            } else {
                " "
            };
            let title_width = (width as isize - task.id.len() as isize - 20).max(12) as usize;
            lines.push(format!(
                "{} {} {} {}",
                selected_marker,
                format_status(task.status),
                style::task_id(&task.id),
                truncate_with_ellipsis(&task.title, title_width)
            ));
            if density == Density::Narrow {
                lines.push(format!("  {}", format_meta_badge(task)));
            }
        }
    }
    lines
}

fn render_inspector(data: &TuiFrameData, width: usize) -> Vec<String> {
    let mut lines = vec![style::heading("inspector")];
    let Some(index) = data.selected_index else {
        lines.push(style::muted("none"));
        return lines;
    };
    let Some(task) = data.tasks.get(index) else {
        lines.push(style::muted("none"));
        return lines;
    };
    let labels = if task.labels.is_empty() {
        "-".to_string()
    } else {
        task.labels.join(",")
    };
    let planning = task
        .planning_state
        .map(planning_state_to_string)
        .unwrap_or("needs_planning");
    lines.push(format!("id={}", style::task_id(&task.id)));
    lines.push(format!(
        "title={}",
        truncate_with_ellipsis(&task.title, width.saturating_sub(8).max(12))
    ));
    lines.push(format!(
        "status={} kind={} priority={} planning={}",
        status_to_string(task.status),
        task_kind_to_string(task.kind),
        task.priority,
        planning
    ));
    lines.push(format!(
        "assignee={} parent={} labels={}",
        task.assignee.as_deref().unwrap_or("-"),
        task.parent_id.as_deref().unwrap_or("-"),
        labels
    ));
    lines.push(format!(
        "updated={} created={}",
        task.updated_at, task.created_at
    ));
    lines
}

fn apply_selection(frame: &mut TuiFrameData, view: TuiView, selected_index: usize) {
    frame.view = view;
    if frame.tasks.is_empty() {
        frame.selected_index = None;
        frame.selected_task_id = None;
        return;
    }
    let clamped = selected_index.min(frame.tasks.len() - 1);
    frame.selected_index = Some(clamped);
    frame.selected_task_id = Some(frame.tasks[clamped].id.clone());
}

fn validate_options(options: &TuiOptions) -> Result<(), TsqError> {
    if options.interval < 1 || options.interval > 60 {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "interval must be between 1 and 60 seconds",
            1,
        ));
    }
    Ok(())
}

fn sort_tui_tasks(mut tasks: Vec<Task>) -> Vec<Task> {
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

fn compute_summary(tasks: &[Task]) -> TuiSummary {
    let mut summary = TuiSummary {
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

fn board_status_order() -> [TaskStatus; 6] {
    [
        TaskStatus::InProgress,
        TaskStatus::Open,
        TaskStatus::Blocked,
        TaskStatus::Deferred,
        TaskStatus::Closed,
        TaskStatus::Canceled,
    ]
}

fn board_bucket_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Open => "OPEN",
        TaskStatus::InProgress => "IN_PROGRESS",
        TaskStatus::Blocked => "BLOCKED",
        TaskStatus::Closed => "CLOSED",
        TaskStatus::Canceled => "CANCELED",
        TaskStatus::Deferred => "DEFERRED",
    }
}

fn view_to_string(view: TuiView) -> &'static str {
    match view {
        TuiView::List => "list",
        TuiView::Board => "board",
    }
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

fn task_kind_to_string(kind: crate::types::TaskKind) -> &'static str {
    match kind {
        crate::types::TaskKind::Task => "task",
        crate::types::TaskKind::Feature => "feature",
        crate::types::TaskKind::Epic => "epic",
    }
}

fn planning_state_to_string(state: crate::types::PlanningState) -> &'static str {
    match state {
        crate::types::PlanningState::NeedsPlanning => "needs_planning",
        crate::types::PlanningState::Planned => "planned",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventState;

    #[test]
    fn down_navigation_ignores_key_release_events() {
        let press = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let release = KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        };

        assert!(is_select_down_key(&press));
        assert!(!is_select_down_key(&release));
    }
}
