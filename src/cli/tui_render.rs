use crate::cli::render::truncate_with_ellipsis;
use crate::cli::style;
use crate::cli::terminal::resolve_width;
use crate::output::{err_envelope, ok_envelope};
use crate::types::{Task, TaskKind, TaskStatus};
use std::io::IsTerminal;

use super::{
    BoardLane, FrameResult, TaskSpecState, TuiEpicProgress, TuiFrameData, TuiTab, tab_from_data,
};

const ANSI_CLEAR: &str = "\x1b[2J\x1b[H";

pub(super) fn output_frame(frame: &FrameResult, json: bool, clear_screen: bool, paused: bool) {
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
            let tab = tab_from_data(data);
            let mut lines = vec![
                render_shell_header(data, paused),
                render_tabs_line(tab),
                render_filter_line(data, width),
                render_summary(data),
                style::muted(&"-".repeat(width)),
            ];
            if data.visible_task_ids.is_empty() {
                lines.push(style::muted("no tasks in current view"));
            } else {
                match tab {
                    TuiTab::Tasks => lines.extend(render_tasks_table(data, width)),
                    TuiTab::Epics => lines.extend(render_epics_view(data, width)),
                    TuiTab::Board => lines.extend(render_board_view(data, width)),
                }
            }
            lines.push(style::muted(&"-".repeat(width)));
            lines.extend(render_inspector(data, width));
            lines.push(style::muted(&"-".repeat(width)));
            if std::io::stdout().is_terminal() {
                lines.push(if paused {
                    style::muted("q quit  Tab view  r refresh  p resume  Up/Down select")
                } else {
                    style::muted("q quit  Tab view  r refresh  p pause  Up/Down select")
                });
            }
            for line in lines {
                println!("{}", line);
            }
        }
    }
}

fn render_shell_header(data: &TuiFrameData, paused: bool) -> String {
    let pause_tag = if paused { "paused" } else { "live" };
    style::heading(&format!(
        "Tasque  refreshed={}  interval={}s  sync={}",
        data.frame_ts, data.interval_s, pause_tag
    ))
}

fn render_tabs_line(tab: TuiTab) -> String {
    let tasks = if tab == TuiTab::Tasks {
        style::heading("[Tasks]")
    } else {
        style::muted("[Tasks]")
    };
    let epics = if tab == TuiTab::Epics {
        style::heading("[Epics]")
    } else {
        style::muted("[Epics]")
    };
    let board = if tab == TuiTab::Board {
        style::heading("[Board]")
    } else {
        style::muted("[Board]")
    };
    format!(
        "{} {} {} {} {}",
        tasks,
        epics,
        board,
        style::muted("[Ready]"),
        style::muted("[History]")
    )
}

fn render_filter_line(data: &TuiFrameData, width: usize) -> String {
    let text = format!(
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
            .map(|assignee| format!(" assignee:{}", assignee))
            .unwrap_or_default()
    );
    format!(
        "filter: {}",
        truncate_with_ellipsis(&text, width.saturating_sub(8).max(16))
    )
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

fn render_tasks_table(data: &TuiFrameData, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(style::heading("tasks"));
    lines.push(render_table_header());

    let title_width = table_title_width(width);
    for task in visible_tasks(data) {
        lines.push(render_table_row(
            task,
            data.selected_task_id.as_deref() == Some(task.id.as_str()),
            title_width,
        ));
    }
    lines
}

fn render_epics_view(data: &TuiFrameData, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(style::heading("epics"));

    if let Some(progress) = data.epic_progress.as_ref() {
        lines.push(render_epic_progress(progress, width));
        lines.push(style::muted(&"-".repeat(width)));
    }

    lines.push(render_table_header());
    let title_width = table_title_width(width);
    for task in visible_tasks(data) {
        lines.push(render_table_row(
            task,
            data.selected_task_id.as_deref() == Some(task.id.as_str()),
            title_width,
        ));
    }

    lines
}

fn render_epic_progress(progress: &TuiEpicProgress, width: usize) -> String {
    let meter = render_progress_meter(progress.done, progress.total, 12);
    let summary = format!(
        "progress: {} {} {} {}/{} open={} in_progress={}",
        progress.epic_id,
        truncate_with_ellipsis(&progress.epic_title, 24),
        meter,
        progress.done,
        progress.total,
        progress.open,
        progress.in_progress
    );
    truncate_with_ellipsis(&summary, width.max(24))
}

fn render_board_view(data: &TuiFrameData, width: usize) -> Vec<String> {
    let mut open_cards = Vec::new();
    let mut in_progress_cards = Vec::new();
    let mut done_cards = Vec::new();

    for task in visible_tasks(data) {
        match board_lane_for_status(task.status) {
            BoardLane::Open => open_cards.push(render_board_card(task)),
            BoardLane::InProgress => in_progress_cards.push(render_board_card(task)),
            BoardLane::Done => done_cards.push(render_board_card(task)),
        }
    }

    let col_width = ((width.saturating_sub(6)) / 3).max(20);
    let mut lines = Vec::new();
    lines.push(style::heading("board"));
    lines.push(format!(
        "{} | {} | {}",
        pad_to_width("Open", col_width),
        pad_to_width("In Progress", col_width),
        pad_to_width("Done", col_width)
    ));

    let rows = open_cards
        .len()
        .max(in_progress_cards.len())
        .max(done_cards.len());
    for idx in 0..rows {
        let open = open_cards.get(idx).map(String::as_str).unwrap_or("");
        let in_progress = in_progress_cards.get(idx).map(String::as_str).unwrap_or("");
        let done = done_cards.get(idx).map(String::as_str).unwrap_or("");
        lines.push(format!(
            "{} | {} | {}",
            pad_to_width(open, col_width),
            pad_to_width(in_progress, col_width),
            pad_to_width(done, col_width)
        ));
    }

    lines
}

fn render_board_card(task: &Task) -> String {
    let title = truncate_with_ellipsis(&task.title, 18);
    format!(
        "{} {} {} {}",
        style::task_id(&task.id),
        status_pill(task.status),
        priority_pill(task.priority),
        spec_pill(task)
    ) + &format!(" {}", title)
}

fn render_inspector(data: &TuiFrameData, width: usize) -> Vec<String> {
    let mut lines = vec![style::heading("inspector")];
    let Some(task_id) = data.selected_task_id.as_deref() else {
        lines.push(style::muted("none"));
        return lines;
    };
    let Some(task) = data.tasks.iter().find(|task| task.id == task_id) else {
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
        task.assignee.as_deref().unwrap_or("unassigned"),
        task.parent_id.as_deref().unwrap_or("-"),
        labels
    ));
    lines.push(format!(
        "updated={} created={}",
        task.updated_at, task.created_at
    ));
    lines.push(render_spec_inspector_line(task, width));
    lines
}

fn render_spec_inspector_line(task: &Task, width: usize) -> String {
    let spec_value = match spec_state(task) {
        TaskSpecState::Attached => {
            let path = task.spec_path.as_deref().unwrap_or("-");
            let fingerprint =
                short_spec_fingerprint(task.spec_fingerprint.as_deref().unwrap_or("-"));
            format!("attached {} ({})", path, fingerprint)
        }
        TaskSpecState::Missing => "missing".to_string(),
        TaskSpecState::InvalidMetadata => "invalid metadata".to_string(),
    };
    let max_width = width.saturating_sub(6).max(16);
    format!("spec={}", truncate_with_ellipsis(&spec_value, max_width))
}

fn task_kind_to_string(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::Task => "task",
        TaskKind::Feature => "feature",
        TaskKind::Epic => "epic",
    }
}

fn type_pill(kind: TaskKind) -> String {
    format!("[{}]", task_kind_to_string(kind))
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

fn status_pill(status: TaskStatus) -> String {
    format!("[{}]", status_to_string(status))
}

fn priority_pill(priority: u8) -> String {
    format!("[P{}]", priority)
}

fn planning_state_to_string(state: crate::types::PlanningState) -> &'static str {
    match state {
        crate::types::PlanningState::NeedsPlanning => "needs_planning",
        crate::types::PlanningState::Planned => "planned",
    }
}

fn spec_state(task: &Task) -> TaskSpecState {
    match (task.spec_path.as_deref(), task.spec_fingerprint.as_deref()) {
        (Some(_), Some(_)) => TaskSpecState::Attached,
        (None, None) => TaskSpecState::Missing,
        _ => TaskSpecState::InvalidMetadata,
    }
}

fn spec_state_label(task: &Task) -> &'static str {
    match spec_state(task) {
        TaskSpecState::Attached => "attached",
        TaskSpecState::Missing => "missing",
        TaskSpecState::InvalidMetadata => "invalid",
    }
}

fn spec_pill(task: &Task) -> String {
    format!("[{}]", spec_state_label(task))
}

fn short_spec_fingerprint(fingerprint: &str) -> &str {
    const MAX_FINGERPRINT_CHARS: usize = 12;
    if fingerprint.chars().count() <= MAX_FINGERPRINT_CHARS {
        return fingerprint;
    }
    let mut byte_end = 0usize;
    for (index, ch) in fingerprint.char_indices().take(MAX_FINGERPRINT_CHARS) {
        byte_end = index + ch.len_utf8();
    }
    &fingerprint[..byte_end]
}

fn board_lane_for_status(status: TaskStatus) -> BoardLane {
    match status {
        TaskStatus::Open | TaskStatus::Deferred => BoardLane::Open,
        TaskStatus::InProgress | TaskStatus::Blocked => BoardLane::InProgress,
        TaskStatus::Closed | TaskStatus::Canceled => BoardLane::Done,
    }
}

fn render_table_header() -> String {
    format!(
        "  {:<12} {:<8} {:<24} {:<13} {:<12} {:<9} {:<9}",
        "ID", "Type", "Title", "Status", "Assignee", "Priority", "Spec"
    )
}

fn table_title_width(width: usize) -> usize {
    width
        .saturating_sub(2 + 12 + 8 + 13 + 12 + 9 + 9 + 12)
        .max(16)
}

fn render_table_row(task: &Task, selected: bool, title_width: usize) -> String {
    let marker = if selected { ">" } else { " " };
    let assignee = task.assignee.as_deref().unwrap_or("unassigned");
    format!(
        "{} {:<12} {:<8} {:<24} {:<13} {:<12} {:<9} {:<9}",
        marker,
        task.id,
        type_pill(task.kind),
        truncate_with_ellipsis(&task.title, title_width),
        status_pill(task.status),
        truncate_with_ellipsis(assignee, 12),
        priority_pill(task.priority),
        spec_pill(task),
    )
}

fn visible_tasks(data: &TuiFrameData) -> Vec<&Task> {
    data.visible_task_ids
        .iter()
        .filter_map(|id| data.tasks.iter().find(|task| task.id == *id))
        .collect()
}

fn render_progress_meter(done: usize, total: usize, width: usize) -> String {
    if total == 0 {
        return format!("[{}]", "░".repeat(width));
    }
    let filled = ((done as f64 / total as f64) * width as f64)
        .round()
        .clamp(0.0, width as f64) as usize;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(width - filled))
}

fn pad_to_width(value: &str, width: usize) -> String {
    let len = value.chars().count();
    if len >= width {
        return truncate_with_ellipsis(value, width);
    }
    format!("{}{}", value, " ".repeat(width - len))
}
