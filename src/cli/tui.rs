use crate::app::service::TasqueService;
use crate::app::service_types::ListFilter;
use crate::cli::render::truncate_with_ellipsis;
use crate::cli::style;
use crate::cli::terminal::resolve_width;
use crate::errors::TsqError;
use crate::output::{err_envelope, ok_envelope};
use crate::types::{Task, TaskKind, TaskStatus};
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
    Epics,
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
pub struct TuiEpicProgress {
    pub epic_id: String,
    pub epic_title: String,
    pub done: usize,
    pub total: usize,
    pub open: usize,
    pub in_progress: usize,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_epic_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epic_progress: Option<TuiEpicProgress>,
    #[serde(skip_serializing, skip_deserializing, default)]
    visible_task_ids: Vec<String>,
}

enum FrameResult {
    Ok(Box<TuiFrameData>),
    Err {
        error: String,
        code: String,
        exit_code: i32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskSpecState {
    Attached,
    Missing,
    InvalidMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TuiTab {
    Tasks,
    Epics,
    Board,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoardLane {
    Open,
    InProgress,
    Done,
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
    let mut tab = initial_tab(options.view);
    let mut paused = false;
    let mut selected_index = 0usize;
    let interval = Duration::from_secs(options.interval as u64);
    let mut last_good_frame: Option<TuiFrameData> = None;

    if options.once {
        let frame = load_frame(service, &options, tab, selected_index);
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
        tab,
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
                                tab,
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
                                    &FrameResult::Ok(Box::new(frame)),
                                    options.json,
                                    can_clear,
                                    paused,
                                );
                            }
                            continue;
                        }
                        if is_switch_view_key(&key) {
                            tab = cycle_tab(tab);
                            selected_index = 0;
                            refresh_frame(
                                service,
                                &options,
                                tab,
                                &mut selected_index,
                                can_clear,
                                paused,
                                &mut last_good_frame,
                            );
                            continue;
                        }
                        if (is_select_up_key(&key) || is_select_down_key(&key))
                            && let Some(frame) = last_good_frame.as_mut()
                        {
                            let visible_count = frame.visible_task_ids.len();
                            if visible_count > 0 {
                                if is_select_up_key(&key) {
                                    selected_index = selected_index.saturating_sub(1);
                                } else {
                                    selected_index = (selected_index + 1).min(visible_count - 1);
                                }
                                apply_selection(frame, selected_index);
                                output_frame(
                                    &FrameResult::Ok(Box::new(frame.clone())),
                                    options.json,
                                    can_clear,
                                    paused,
                                );
                            }
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
                            tab,
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
            tab,
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
    tab: TuiTab,
    selected_index: &mut usize,
    clear_screen: bool,
    paused: bool,
    last_good_frame: &mut Option<TuiFrameData>,
) {
    match load_frame(service, options, tab, *selected_index) {
        FrameResult::Ok(data) => {
            let data = *data;
            if let Some(index) = data.selected_index {
                *selected_index = index;
            } else {
                *selected_index = 0;
            }
            *last_good_frame = Some(data.clone());
            output_frame(
                &FrameResult::Ok(Box::new(data)),
                options.json,
                clear_screen,
                paused,
            );
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
                    &FrameResult::Ok(Box::new(previous)),
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

fn initial_tab(view: TuiView) -> TuiTab {
    match view {
        TuiView::List => TuiTab::Tasks,
        TuiView::Epics => TuiTab::Epics,
        TuiView::Board => TuiTab::Board,
    }
}

fn cycle_tab(tab: TuiTab) -> TuiTab {
    match tab {
        TuiTab::Tasks => TuiTab::Epics,
        TuiTab::Epics => TuiTab::Board,
        TuiTab::Board => TuiTab::Tasks,
    }
}

fn tab_to_view(tab: TuiTab) -> TuiView {
    match tab {
        TuiTab::Tasks => TuiView::List,
        TuiTab::Epics => TuiView::Epics,
        TuiTab::Board => TuiView::Board,
    }
}

fn tab_to_string(tab: TuiTab) -> &'static str {
    match tab {
        TuiTab::Tasks => "tasks",
        TuiTab::Epics => "epics",
        TuiTab::Board => "board",
    }
}

fn tab_from_data(data: &TuiFrameData) -> TuiTab {
    match data.tab.as_deref() {
        Some("epics") => TuiTab::Epics,
        Some("board") => TuiTab::Board,
        _ => TuiTab::Tasks,
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
    tab: TuiTab,
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
            let (visible_task_ids, selected_epic_id, epic_progress) =
                build_view_state(tab, &sorted);
            let selected = if visible_task_ids.is_empty() {
                None
            } else {
                Some(selected_index.min(visible_task_ids.len() - 1))
            };
            let selected_task_id = selected.and_then(|index| visible_task_ids.get(index).cloned());
            FrameResult::Ok(Box::new(TuiFrameData {
                frame_ts: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                interval_s: options.interval,
                view: tab_to_view(tab),
                filters: TuiFrameFilters {
                    status: options.statuses.clone(),
                    assignee: options.assignee.clone(),
                },
                summary,
                tasks: sorted,
                selected_index: selected,
                selected_task_id,
                tab: Some(tab_to_string(tab).to_string()),
                selected_epic_id,
                epic_progress,
                visible_task_ids,
            }))
        }
        Err(error) => FrameResult::Err {
            error: error.message,
            code: error.code,
            exit_code: error.exit_code,
        },
    }
}

fn build_view_state(
    tab: TuiTab,
    tasks: &[Task],
) -> (Vec<String>, Option<String>, Option<TuiEpicProgress>) {
    match tab {
        TuiTab::Tasks | TuiTab::Board => (
            tasks.iter().map(|task| task.id.clone()).collect(),
            None,
            None,
        ),
        TuiTab::Epics => {
            let epics: Vec<&Task> = tasks
                .iter()
                .filter(|task| task.kind == TaskKind::Epic)
                .collect();
            if epics.is_empty() {
                return (Vec::new(), None, None);
            }

            let selected_epic = epics[0];
            let children: Vec<&Task> = tasks
                .iter()
                .filter(|task| task.parent_id.as_deref() == Some(selected_epic.id.as_str()))
                .collect();
            let visible_task_ids = if children.is_empty() {
                epics.iter().map(|task| task.id.clone()).collect()
            } else {
                children.iter().map(|task| task.id.clone()).collect()
            };

            let mut done = 0usize;
            let mut open = 0usize;
            let mut in_progress = 0usize;
            for task in &children {
                match task.status {
                    TaskStatus::Closed | TaskStatus::Canceled => done += 1,
                    TaskStatus::InProgress | TaskStatus::Blocked => in_progress += 1,
                    _ => open += 1,
                }
            }

            (
                visible_task_ids,
                Some(selected_epic.id.clone()),
                Some(TuiEpicProgress {
                    epic_id: selected_epic.id.clone(),
                    epic_title: selected_epic.title.clone(),
                    done,
                    total: children.len(),
                    open,
                    in_progress,
                }),
            )
        }
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

fn apply_selection(frame: &mut TuiFrameData, selected_index: usize) {
    if frame.visible_task_ids.is_empty() {
        frame.selected_index = None;
        frame.selected_task_id = None;
        return;
    }
    let clamped = selected_index.min(frame.visible_task_ids.len() - 1);
    frame.selected_index = Some(clamped);
    frame.selected_task_id = frame.visible_task_ids.get(clamped).cloned();
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
