use crate::app::service::TasqueService;
use crate::types::{Task, TaskStatus};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use serde::{Deserialize, Serialize};
use std::io::IsTerminal;
use std::thread;
use std::time::Duration;

#[path = "tui_data.rs"]
mod tui_data;
#[path = "tui_model.rs"]
mod tui_model;
#[path = "tui_render.rs"]
mod tui_render;
use tui_data::load_frame;
use tui_model::apply_selection;
use tui_model::validate_options;
use tui_render::output_frame;

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
    let mut tab = options.view;
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
                    Ok(Event::Key(key)) => match classify_key(&key) {
                        Some(TuiKeyAction::Quit) => break,
                        Some(TuiKeyAction::Refresh) => refresh_frame(
                            service,
                            &options,
                            tab,
                            &mut selected_index,
                            can_clear,
                            paused,
                            &mut last_good_frame,
                        ),
                        Some(TuiKeyAction::TogglePause) => {
                            paused = !paused;
                            if let Some(frame) = last_good_frame.clone() {
                                output_frame(
                                    &FrameResult::Ok(Box::new(frame)),
                                    options.json,
                                    can_clear,
                                    paused,
                                );
                            }
                        }
                        Some(TuiKeyAction::SwitchView) => {
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
                        }
                        Some(TuiKeyAction::Select(delta)) => {
                            if let Some(frame) = last_good_frame.as_mut() {
                                let visible_count = frame.visible_task_ids.len();
                                if visible_count > 0 {
                                    selected_index = match delta {
                                        SelectionDelta::Up => selected_index.saturating_sub(1),
                                        SelectionDelta::Down => {
                                            (selected_index + 1).min(visible_count - 1)
                                        }
                                    };
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
                        None => {}
                    },
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
    tab: TuiView,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionDelta {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TuiKeyAction {
    Quit,
    Refresh,
    TogglePause,
    SwitchView,
    Select(SelectionDelta),
}

fn classify_key(key: &KeyEvent) -> Option<TuiKeyAction> {
    if !is_press_like(key) {
        return None;
    }
    match key.code {
        KeyCode::Char(value) if value.eq_ignore_ascii_case(&'q') => Some(TuiKeyAction::Quit),
        KeyCode::Char(value)
            if value.eq_ignore_ascii_case(&'c')
                && key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(TuiKeyAction::Quit)
        }
        KeyCode::Char(value) if value.eq_ignore_ascii_case(&'r') => Some(TuiKeyAction::Refresh),
        KeyCode::Char(value) if value.eq_ignore_ascii_case(&'p') => Some(TuiKeyAction::TogglePause),
        KeyCode::Tab => Some(TuiKeyAction::SwitchView),
        KeyCode::Up => Some(TuiKeyAction::Select(SelectionDelta::Up)),
        KeyCode::Down => Some(TuiKeyAction::Select(SelectionDelta::Down)),
        _ => None,
    }
}

fn is_press_like(key: &KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

fn cycle_tab(tab: TuiView) -> TuiView {
    match tab {
        TuiView::List => TuiView::Epics,
        TuiView::Epics => TuiView::Board,
        TuiView::Board => TuiView::List,
    }
}

fn tab_to_string(tab: TuiView) -> &'static str {
    match tab {
        TuiView::List => "tasks",
        TuiView::Epics => "epics",
        TuiView::Board => "board",
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

        assert_eq!(
            classify_key(&press),
            Some(TuiKeyAction::Select(SelectionDelta::Down))
        );
        assert_eq!(classify_key(&release), None);
    }
}
