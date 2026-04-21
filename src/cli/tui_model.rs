use crate::errors::TsqError;
use crate::types::{Task, TaskStatus};

use super::{TuiFrameData, TuiOptions, TuiSummary};

pub(super) fn apply_selection(frame: &mut TuiFrameData, selected_index: usize) {
    if frame.visible_task_ids.is_empty() {
        frame.selected_index = None;
        frame.selected_task_id = None;
        return;
    }
    let clamped = selected_index.min(frame.visible_task_ids.len() - 1);
    frame.selected_index = Some(clamped);
    frame.selected_task_id = frame.visible_task_ids.get(clamped).cloned();
}

pub(super) fn validate_options(options: &TuiOptions) -> Result<(), TsqError> {
    if options.interval < 1 || options.interval > 60 {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "interval must be between 1 and 60 seconds",
            1,
        ));
    }
    Ok(())
}

pub(super) fn sort_tui_tasks(mut tasks: Vec<Task>) -> Vec<Task> {
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

pub(super) fn compute_summary(tasks: &[Task]) -> TuiSummary {
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
