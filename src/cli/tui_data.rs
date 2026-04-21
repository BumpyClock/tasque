use crate::app::service::TasqueService;
use crate::app::service_types::ListFilter;
use crate::types::{Task, TaskKind, TaskStatus};
use chrono::Utc;

use super::tui_model::{compute_summary, sort_tui_tasks};
use super::{
    FrameResult, TuiEpicProgress, TuiFrameData, TuiFrameFilters, TuiOptions, TuiTab, tab_to_string,
    tab_to_view,
};

pub(super) fn load_frame(
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
