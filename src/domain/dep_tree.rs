use crate::domain::deps::normalize_dependency_edges;
use crate::errors::TsqError;
use crate::types::{DependencyEdge, DependencyType, State, Task};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Direction to walk the dependency graph from the root task.
/// Example: DepDirection::Up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepDirection {
    Up,
    Down,
    Both,
}

/// A node in a dependency tree with task data and traversal metadata.
/// Example: build_dep_tree returns a DepTreeNode for the requested root task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepTreeNode {
    pub id: String,
    pub task: Task,
    pub direction: DepDirection,
    pub depth: usize,
    pub dep_type: Option<DependencyType>,
    pub children: Vec<DepTreeNode>,
}

/// A dependent edge used when building a reverse index.
/// Example: build_dependents_by_blocker collects Dependents for each blocker.
#[derive(Debug, Clone)]
pub struct DependentEdge {
    pub id: String,
    pub dep_type: DependencyType,
}

/// Build a dependency tree starting from root_id.
/// Example: build_dep_tree(state, "tsq-123", DepDirection::Both, 10).
pub fn build_dep_tree(
    state: &State,
    root_id: &str,
    direction: DepDirection,
    max_depth: usize,
) -> Result<DepTreeNode, TsqError> {
    let root_task = match state.tasks.get(root_id) {
        Some(task) => task.clone(),
        None => {
            return Err(TsqError::new(
                "NOT_FOUND",
                format!("task not found: {}", root_id),
                1,
            ));
        }
    };

    let dependents_by_blocker = build_dependents_by_blocker(&state.deps);

    match direction {
        DepDirection::Both => {
            let mut visited_up = HashSet::new();
            visited_up.insert(root_id.to_string());
            let mut visited_down = HashSet::new();
            visited_down.insert(root_id.to_string());
            let up_children = walk_up(
                state,
                root_id,
                1,
                max_depth,
                &mut visited_up,
                DepDirection::Up,
            );
            let down_children = walk_down(
                &dependents_by_blocker,
                state,
                root_id,
                1,
                max_depth,
                &mut visited_down,
                DepDirection::Down,
            );
            let mut children = Vec::with_capacity(up_children.len() + down_children.len());
            children.extend(up_children);
            children.extend(down_children);
            Ok(DepTreeNode {
                id: root_id.to_string(),
                task: root_task,
                direction: DepDirection::Both,
                depth: 0,
                dep_type: None,
                children,
            })
        }
        DepDirection::Up => {
            let mut visited = HashSet::new();
            visited.insert(root_id.to_string());
            Ok(DepTreeNode {
                id: root_id.to_string(),
                task: root_task,
                direction: DepDirection::Up,
                depth: 0,
                dep_type: None,
                children: walk_up(state, root_id, 1, max_depth, &mut visited, DepDirection::Up),
            })
        }
        DepDirection::Down => {
            let mut visited = HashSet::new();
            visited.insert(root_id.to_string());
            Ok(DepTreeNode {
                id: root_id.to_string(),
                task: root_task,
                direction: DepDirection::Down,
                depth: 0,
                dep_type: None,
                children: walk_down(
                    &dependents_by_blocker,
                    state,
                    root_id,
                    1,
                    max_depth,
                    &mut visited,
                    DepDirection::Down,
                ),
            })
        }
    }
}

fn walk_up(
    state: &State,
    node_id: &str,
    depth: usize,
    max_depth: usize,
    visited: &mut HashSet<String>,
    direction: DepDirection,
) -> Vec<DepTreeNode> {
    if depth > max_depth {
        return Vec::new();
    }
    let blockers = normalize_dependency_edges(state.deps.get(node_id));
    let mut nodes = Vec::new();
    for edge in blockers {
        let blocker_id = edge.blocker;
        if visited.contains(&blocker_id) {
            continue;
        }
        let blocker_task = match state.tasks.get(&blocker_id) {
            Some(task) => task.clone(),
            None => continue,
        };
        visited.insert(blocker_id.clone());
        nodes.push(DepTreeNode {
            id: blocker_id.clone(),
            task: blocker_task,
            direction,
            depth,
            dep_type: Some(edge.dep_type),
            children: walk_up(state, &blocker_id, depth + 1, max_depth, visited, direction),
        });
    }
    nodes
}

fn walk_down(
    dependents_by_blocker: &HashMap<String, Vec<DependentEdge>>,
    state: &State,
    node_id: &str,
    depth: usize,
    max_depth: usize,
    visited: &mut HashSet<String>,
    direction: DepDirection,
) -> Vec<DepTreeNode> {
    if depth > max_depth {
        return Vec::new();
    }
    let dependents = dependents_by_blocker.get(node_id);
    let mut nodes = Vec::new();
    if let Some(edges) = dependents {
        for dependent in edges {
            let dependent_id = dependent.id.clone();
            if visited.contains(&dependent_id) {
                continue;
            }
            let dependent_task = match state.tasks.get(&dependent_id) {
                Some(task) => task.clone(),
                None => continue,
            };
            visited.insert(dependent_id.clone());
            nodes.push(DepTreeNode {
                id: dependent_id.clone(),
                task: dependent_task,
                direction,
                depth,
                dep_type: Some(dependent.dep_type),
                children: walk_down(
                    dependents_by_blocker,
                    state,
                    &dependent_id,
                    depth + 1,
                    max_depth,
                    visited,
                    direction,
                ),
            });
        }
    }
    nodes
}

/// Build a reverse index mapping blockers to their dependents.
/// Example: build_dependents_by_blocker(&state.deps).
pub fn build_dependents_by_blocker(
    deps: &HashMap<String, Vec<DependencyEdge>>,
) -> HashMap<String, Vec<DependentEdge>> {
    let mut map: HashMap<String, Vec<DependentEdge>> = HashMap::new();
    for (child, blockers) in deps {
        for edge in normalize_dependency_edges(Some(blockers)) {
            let dependent = DependentEdge {
                id: child.clone(),
                dep_type: edge.dep_type,
            };
            map.entry(edge.blocker)
                .and_modify(|list| list.push(dependent.clone()))
                .or_insert_with(|| vec![dependent]);
        }
    }
    map
}
