use crate::app::service_types::ListFilter;
use crate::domain::ids::make_root_id;
use crate::domain::resolve::resolve_task_id;
use crate::errors::TsqError;
use crate::types::{RelationType, State, Task, TaskStatus};
use regex::Regex;
use ulid::Ulid;

pub const DEFAULT_STALE_STATUSES: &[TaskStatus] = &[
    TaskStatus::Open,
    TaskStatus::InProgress,
    TaskStatus::Blocked,
    TaskStatus::Deferred,
];

pub fn unique_root_id(state: &State, title: &str) -> Result<String, TsqError> {
    let max_attempts = 10;
    for idx in 0..max_attempts {
        let nonce = if idx == 0 {
            None
        } else {
            Some(Ulid::new().to_string())
        };
        let id = make_root_id(Some(title), nonce.as_deref());
        if !state.tasks.contains_key(&id) {
            return Ok(id);
        }
    }
    Err(TsqError::new(
        "ID_COLLISION",
        "unable to allocate unique task id",
        2,
    ))
}

pub fn must_task(state: &State, id: &str) -> Result<Task, TsqError> {
    state
        .tasks
        .get(id)
        .cloned()
        .ok_or_else(|| TsqError::new("NOT_FOUND", format!("task not found: {}", id), 1))
}

pub fn must_resolve_existing(state: &State, raw: &str, exact_id: bool) -> Result<String, TsqError> {
    let id = resolve_task_id(state, raw, exact_id)?;
    if !state.tasks.contains_key(&id) {
        return Err(TsqError::new(
            "NOT_FOUND",
            format!("task not found: {}", raw),
            1,
        ));
    }
    Ok(id)
}

pub fn sort_tasks(tasks: &[Task]) -> Vec<Task> {
    let mut sorted = tasks.to_vec();
    sorted.sort_by(|a, b| {
        if a.priority != b.priority {
            return a.priority.cmp(&b.priority);
        }
        if a.created_at == b.created_at {
            return a.id.cmp(&b.id);
        }
        a.created_at.cmp(&b.created_at)
    });
    sorted
}

pub fn sort_stale_tasks(tasks: &[Task]) -> Vec<Task> {
    let mut sorted = tasks.to_vec();
    sorted.sort_by(|a, b| {
        if a.updated_at != b.updated_at {
            return a.updated_at.cmp(&b.updated_at);
        }
        if a.priority != b.priority {
            return a.priority.cmp(&b.priority);
        }
        a.id.cmp(&b.id)
    });
    sorted
}

pub fn apply_list_filter(tasks: &[Task], filter: &ListFilter) -> Vec<Task> {
    tasks
        .iter()
        .filter(|task| {
            if let Some(statuses) = &filter.statuses
                && !statuses.contains(&task.status)
            {
                return false;
            }
            if let Some(ids) = &filter.ids
                && !ids.contains(&task.id)
            {
                return false;
            }
            if let Some(assignee) = &filter.assignee
                && task.assignee.as_deref() != Some(assignee.as_str())
            {
                return false;
            }
            if let Some(external_ref) = &filter.external_ref
                && task.external_ref.as_deref() != Some(external_ref.as_str())
            {
                return false;
            }
            if let Some(discovered_from) = &filter.discovered_from
                && task.discovered_from.as_deref() != Some(discovered_from.as_str())
            {
                return false;
            }
            if filter.unassigned && has_assignee(task.assignee.as_deref()) {
                return false;
            }
            if let Some(kind) = &filter.kind
                && &task.kind != kind
            {
                return false;
            }
            if let Some(label) = &filter.label
                && !task.labels.iter().any(|value| value == label)
            {
                return false;
            }
            if let Some(labels) = &filter.label_any
                && !labels
                    .iter()
                    .any(|label| task.labels.iter().any(|value| value == label))
            {
                return false;
            }
            if let Some(created_after) = &filter.created_after
                && task.created_at <= *created_after
            {
                return false;
            }
            if let Some(updated_after) = &filter.updated_after
                && task.updated_at <= *updated_after
            {
                return false;
            }
            if let Some(closed_after) = &filter.closed_after {
                if let Some(closed_at) = &task.closed_at {
                    if closed_at <= closed_after {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            if let Some(planning_state) = &filter.planning_state
                && task.planning_state.as_ref() != Some(planning_state)
            {
                return false;
            }
            true
        })
        .cloned()
        .collect()
}

fn has_assignee(value: Option<&str>) -> bool {
    value.map(|value| !value.trim().is_empty()).unwrap_or(false)
}

pub fn has_duplicate_link(state: &State, source: &str, canonical: &str) -> bool {
    state
        .links
        .get(source)
        .and_then(|rels| rels.get(&RelationType::Duplicates))
        .map(|targets| targets.iter().any(|value| value == canonical))
        .unwrap_or(false)
}

pub fn creates_duplicate_cycle(state: &State, source: &str, canonical: &str) -> bool {
    let mut visited = std::collections::HashSet::new();
    let mut cursor = Some(canonical.to_string());
    while let Some(current) = cursor {
        if current == source {
            return true;
        }
        if visited.contains(&current) {
            return false;
        }
        visited.insert(current.clone());
        cursor = state
            .tasks
            .get(&current)
            .and_then(|task| task.duplicate_of.clone());
    }
    false
}

pub fn normalize_duplicate_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let non_alnum = Regex::new(r"[^a-z0-9]+")
        .ok()
        .map(|regex| regex.replace_all(&lower, " ").to_string())
        .unwrap_or(lower);
    let collapsed = Regex::new(r"\s+")
        .ok()
        .map(|regex| regex.replace_all(&non_alnum, " ").to_string())
        .unwrap_or(non_alnum);
    collapsed.trim().to_string()
}

pub fn sort_task_ids(task_ids: &[String]) -> Vec<String> {
    let mut sorted = task_ids.to_vec();
    sorted.sort();
    sorted
}
