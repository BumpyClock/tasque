use crate::domain::deps::normalize_dependency_edges;
use crate::errors::TsqError;
use crate::types::{DependencyType, PlanningState, State, Task, TaskStatus};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanningLane {
    Planning,
    Coding,
}

fn blocking_dep_ids(state: &State, task_id: &str) -> Vec<String> {
    normalize_dependency_edges(state.deps.get(task_id))
        .into_iter()
        .filter(|edge| edge.dep_type == DependencyType::Blocks)
        .map(|edge| edge.blocker)
        .collect()
}

pub fn assert_no_dependency_cycle(
    state: &State,
    child: &str,
    blocker: &str,
) -> Result<(), TsqError> {
    if child == blocker {
        return Err(
            TsqError::new("DEPENDENCY_CYCLE", "Dependency cycle detected", 1).with_details(json!({
              "child": child,
              "blocker": blocker
            })),
        );
    }

    let mut stack = vec![blocker.to_string()];
    let mut visited: HashSet<String> = HashSet::new();
    while let Some(current) = stack.pop() {
        if visited.contains(&current) {
            continue;
        }
        if current == child {
            return Err(
                TsqError::new("DEPENDENCY_CYCLE", "Dependency cycle detected", 1).with_details(
                    json!({
                      "child": child,
                      "blocker": blocker
                    }),
                ),
            );
        }
        visited.insert(current.clone());
        for next in blocking_dep_ids(state, &current) {
            if !visited.contains(&next) {
                stack.push(next);
            }
        }
    }

    Ok(())
}

pub fn is_ready(state: &State, task_id: &str) -> bool {
    let Some(task) = state.tasks.get(task_id) else {
        return false;
    };
    if !matches!(task.status, TaskStatus::Open | TaskStatus::InProgress) {
        return false;
    }

    for blocker_id in blocking_dep_ids(state, task_id) {
        let Some(blocker) = state.tasks.get(&blocker_id) else {
            return false;
        };
        if !matches!(blocker.status, TaskStatus::Closed | TaskStatus::Canceled) {
            return false;
        }
    }

    true
}

pub fn list_ready(state: &State) -> Vec<Task> {
    let mut ready = Vec::new();

    for id in &state.created_order {
        let Some(task) = state.tasks.get(id) else {
            continue;
        };
        if is_ready(state, id) {
            ready.push(task.clone());
        }
    }

    ready
}

pub fn list_ready_by_lane(state: &State, lane: Option<PlanningLane>) -> Vec<Task> {
    let all = list_ready(state);
    let Some(lane) = lane else {
        return all;
    };
    match lane {
        PlanningLane::Planning => all
            .into_iter()
            .filter(|task| {
                matches!(
                    task.planning_state,
                    None | Some(PlanningState::NeedsPlanning)
                )
            })
            .collect(),
        PlanningLane::Coding => all
            .into_iter()
            .filter(|task| matches!(task.planning_state, Some(PlanningState::Planned)))
            .collect(),
    }
}
