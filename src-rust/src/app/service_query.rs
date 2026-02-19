use crate::app::repair::scan_orphaned_graph;
use crate::app::service_types::{
    DepDirectionFilter, DoctorResult, HistoryInput, HistoryResult, ListFilter, OrphanedLinkResult,
    OrphansResult, SearchInput, ServiceContext, StaleInput, StaleResult,
};
use crate::app::service_utils::{
    DEFAULT_STALE_STATUSES, apply_list_filter, must_resolve_existing, must_task, sort_stale_tasks,
    sort_task_ids, sort_tasks,
};
use crate::app::storage::load_projected_state;
use crate::domain::dep_tree::build_dependents_by_blocker;
use crate::domain::deps::normalize_dependency_edges;
use crate::domain::query::{evaluate_query, parse_query};
use crate::domain::validate::{PlanningLane, is_ready, list_ready, list_ready_by_lane};
use crate::errors::TsqError;
use crate::types::{
    DependencyRef, DependencyType, EventRecord, EventType, RelationType, Task, TaskStatus,
    TaskTreeNode,
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowResult {
    pub task: Task,
    pub blockers: Vec<String>,
    pub dependents: Vec<String>,
    pub blocker_edges: Vec<DependencyRef>,
    pub dependent_edges: Vec<DependencyRef>,
    pub ready: bool,
    pub links: HashMap<String, Vec<String>>,
    pub history: Vec<EventRecord>,
}

pub fn show(ctx: &ServiceContext, id_raw: &str, exact_id: bool) -> Result<ShowResult, TsqError> {
    let loaded = load_projected_state(&ctx.repo_root)?;
    let id = must_resolve_existing(&loaded.state, id_raw, exact_id)?;
    let task = must_task(&loaded.state, &id)?;

    let blocker_edges = sort_dependency_refs(
        normalize_dependency_edges(loaded.state.deps.get(&id))
            .into_iter()
            .map(|edge| DependencyRef {
                id: edge.blocker,
                dep_type: edge.dep_type,
            })
            .collect(),
    );
    let dependents_by_blocker = build_dependents_by_blocker(&loaded.state.deps);
    let dependent_edges = sort_dependency_refs(
        dependents_by_blocker
            .get(&id)
            .map(|edges| {
                edges
                    .iter()
                    .map(|edge| DependencyRef {
                        id: edge.id.clone(),
                        dep_type: edge.dep_type,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    );

    let blockers = unique_ids(&blocker_edges);
    let dependents = unique_ids(&dependent_edges);

    let mut links: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(links_raw) = loaded.state.links.get(&id) {
        for (kind, values) in links_raw {
            links.insert(relation_type_to_string(*kind).to_string(), values.clone());
        }
    }

    let history: Vec<EventRecord> = loaded
        .all_events
        .into_iter()
        .filter(|evt| {
            if evt.task_id == id {
                return true;
            }
            for value in evt.payload.values() {
                if let Some(value) = value.as_str() {
                    if value.starts_with("tsq-") && value == id {
                        return true;
                    }
                }
            }
            false
        })
        .collect();

    Ok(ShowResult {
        task,
        blockers,
        dependents,
        blocker_edges,
        dependent_edges,
        ready: is_ready(&loaded.state, &id),
        links,
        history,
    })
}

pub fn list(ctx: &ServiceContext, filter: &ListFilter) -> Result<Vec<Task>, TsqError> {
    let loaded = load_projected_state(&ctx.repo_root)?;
    let base = apply_list_filter(
        &loaded.state.tasks.values().cloned().collect::<Vec<_>>(),
        filter,
    );
    let dep_type = filter.dep_type;
    if dep_type.is_none() {
        return Ok(sort_tasks(&base));
    }
    let direction = filter.dep_direction.unwrap_or(DepDirectionFilter::Any);
    let dependents_by_blocker = build_dependents_by_blocker(&loaded.state.deps);
    let filtered: Vec<Task> = base
        .into_iter()
        .filter(|task| {
            matches_dep_type_filter(
                &loaded.state,
                &dependents_by_blocker,
                &task.id,
                dep_type.unwrap(),
                direction,
            )
        })
        .collect();
    Ok(sort_tasks(&filtered))
}

pub fn stale(ctx: &ServiceContext, input: &StaleInput) -> Result<StaleResult, TsqError> {
    if input.days < 0 {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "days must be an integer >= 0",
            1,
        ));
    }
    if let Some(limit) = input.limit {
        if limit < 1 {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "limit must be an integer >= 1",
                1,
            ));
        }
    }

    let loaded = load_projected_state(&ctx.repo_root)?;
    let now_value = ctx.now.as_ref()();
    let now_dt = DateTime::parse_from_rfc3339(&now_value)
        .map_err(|_| {
            TsqError::new(
                "INTERNAL_ERROR",
                format!("invalid current timestamp: {}", now_value),
                2,
            )
        })?
        .with_timezone(&Utc);
    let cutoff_dt = now_dt - Duration::days(input.days);
    let cutoff = cutoff_dt.to_rfc3339_opts(SecondsFormat::Millis, true);

    let statuses: Vec<TaskStatus> = match input.status {
        Some(status) => vec![status],
        None => DEFAULT_STALE_STATUSES.to_vec(),
    };

    let tasks: Vec<Task> = loaded
        .state
        .tasks
        .values()
        .filter(|task| statuses.contains(&task.status))
        .filter(|task| match input.assignee.as_deref() {
            Some(assignee) => task.assignee.as_deref() == Some(assignee),
            None => true,
        })
        .filter(|task| task.updated_at <= cutoff)
        .cloned()
        .collect();

    let sorted = sort_stale_tasks(&tasks);
    let limited = match input.limit {
        Some(limit) => sorted.into_iter().take(limit).collect(),
        None => sorted,
    };

    Ok(StaleResult {
        tasks: limited,
        days: input.days,
        cutoff,
        statuses,
    })
}

pub fn list_tree(ctx: &ServiceContext, filter: &ListFilter) -> Result<Vec<TaskTreeNode>, TsqError> {
    let loaded = load_projected_state(&ctx.repo_root)?;
    let filtered_tasks = apply_list_filter(
        &loaded.state.tasks.values().cloned().collect::<Vec<_>>(),
        filter,
    );
    let tasks_by_id: HashMap<String, Task> = filtered_tasks
        .iter()
        .cloned()
        .map(|task| (task.id.clone(), task))
        .collect();
    let mut children_by_parent: HashMap<String, Vec<Task>> = HashMap::new();
    let mut roots: Vec<Task> = Vec::new();

    for task in &filtered_tasks {
        if let Some(parent_id) = task.parent_id.as_ref() {
            if tasks_by_id.contains_key(parent_id) {
                children_by_parent
                    .entry(parent_id.clone())
                    .or_default()
                    .push(task.clone());
                continue;
            }
        }
        roots.push(task.clone());
    }

    let dependents_by_blocker = build_dependents_by_blocker(&loaded.state.deps);

    fn build_node(
        task: &Task,
        state: &crate::types::State,
        children_by_parent: &HashMap<String, Vec<Task>>,
        dependents_by_blocker: &HashMap<String, Vec<crate::domain::dep_tree::DependentEdge>>,
    ) -> TaskTreeNode {
        let blocker_edges = sort_dependency_refs(
            normalize_dependency_edges(state.deps.get(&task.id))
                .into_iter()
                .map(|edge| DependencyRef {
                    id: edge.blocker,
                    dep_type: edge.dep_type,
                })
                .collect(),
        );
        let dependent_edges = sort_dependency_refs(
            dependents_by_blocker
                .get(&task.id)
                .map(|edges| {
                    edges
                        .iter()
                        .map(|edge| DependencyRef {
                            id: edge.id.clone(),
                            dep_type: edge.dep_type,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        );
        let blockers = sort_task_ids(&unique_ids(&blocker_edges));
        let dependents = sort_task_ids(&unique_ids(&dependent_edges));
        let child_tasks = sort_tasks(
            children_by_parent
                .get(&task.id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]),
        );
        TaskTreeNode {
            task: task.clone(),
            blockers,
            dependents,
            blocker_edges: Some(blocker_edges),
            dependent_edges: Some(dependent_edges),
            children: child_tasks
                .iter()
                .map(|child| build_node(child, state, children_by_parent, dependents_by_blocker))
                .collect(),
        }
    }

    let mut sorted_roots = sort_tasks(&roots);
    Ok(sorted_roots
        .drain(..)
        .map(|task| {
            build_node(
                &task,
                &loaded.state,
                &children_by_parent,
                &dependents_by_blocker,
            )
        })
        .collect())
}

pub fn ready(ctx: &ServiceContext, lane: Option<PlanningLane>) -> Result<Vec<Task>, TsqError> {
    let loaded = load_projected_state(&ctx.repo_root)?;
    let ready = match lane {
        Some(lane) => list_ready_by_lane(&loaded.state, Some(lane)),
        None => list_ready(&loaded.state),
    };
    Ok(sort_tasks(&ready))
}

pub fn doctor(ctx: &ServiceContext) -> Result<DoctorResult, TsqError> {
    let loaded = load_projected_state(&ctx.repo_root)?;
    let mut issues = Vec::new();

    for (child, blockers) in &loaded.state.deps {
        if !loaded.state.tasks.contains_key(child) {
            issues.push(format!("dependency source missing: {}", child));
        }
        for edge in normalize_dependency_edges(Some(blockers)) {
            if !loaded.state.tasks.contains_key(&edge.blocker) {
                issues.push(format!(
                    "dependency blocker missing: {} -> {} ({})",
                    child,
                    edge.blocker,
                    dep_type_to_string(edge.dep_type)
                ));
            }
        }
    }

    for (src, rels) in &loaded.state.links {
        if !loaded.state.tasks.contains_key(src) {
            issues.push(format!("relation source missing: {}", src));
        }
        for (kind, targets) in rels {
            for target in targets {
                if !loaded.state.tasks.contains_key(target) {
                    issues.push(format!(
                        "relation target missing: {} -[{}]-> {}",
                        src,
                        relation_type_to_string(*kind),
                        target
                    ));
                }
            }
        }
    }

    Ok(DoctorResult {
        tasks: loaded.state.tasks.len(),
        events: loaded.all_events.len(),
        snapshot_loaded: loaded.snapshot.is_some(),
        warning: loaded.warning,
        issues,
    })
}

pub fn history(ctx: &ServiceContext, input: &HistoryInput) -> Result<HistoryResult, TsqError> {
    let loaded = load_projected_state(&ctx.repo_root)?;
    let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;

    let mut events: Vec<EventRecord> = loaded
        .all_events
        .into_iter()
        .filter(|evt| {
            if evt.task_id == id {
                return true;
            }
            for value in evt.payload.values() {
                if let Some(value) = value.as_str() {
                    if value == id {
                        return true;
                    }
                }
            }
            false
        })
        .collect();

    if let Some(event_type) = input.event_type.as_deref() {
        events = events
            .into_iter()
            .filter(|evt| event_type_to_string(evt.event_type) == event_type)
            .collect();
    }
    if let Some(actor) = input.actor.as_deref() {
        events = events
            .into_iter()
            .filter(|evt| evt.actor == actor)
            .collect();
    }
    if let Some(since) = input.since.as_deref() {
        events = events
            .into_iter()
            .filter(|evt| evt.ts.as_str() >= since)
            .collect();
    }

    events.sort_by(|a, b| b.ts.cmp(&a.ts));

    let limit = input.limit.unwrap_or(50);
    let truncated = events.len() > limit;
    let limited = events.into_iter().take(limit).collect::<Vec<_>>();

    Ok(HistoryResult {
        events: limited.clone(),
        count: limited.len(),
        truncated,
    })
}

pub fn search(ctx: &ServiceContext, input: &SearchInput) -> Result<Vec<Task>, TsqError> {
    let loaded = load_projected_state(&ctx.repo_root)?;
    let filter = parse_query(&input.query)?;
    Ok(sort_tasks(&evaluate_query(
        &loaded.state.tasks.values().cloned().collect::<Vec<_>>(),
        &filter,
        &loaded.state,
    )))
}

pub fn orphans(ctx: &ServiceContext) -> Result<OrphansResult, TsqError> {
    let loaded = load_projected_state(&ctx.repo_root)?;
    let scan = scan_orphaned_graph(&loaded.state);
    let orphaned_links: Vec<OrphanedLinkResult> = scan
        .orphaned_links
        .into_iter()
        .map(|link| OrphanedLinkResult {
            src: link.src,
            dst: link.dst,
            rel_type: relation_type_to_string(link.rel_type).to_string(),
        })
        .collect();
    let total = scan.orphaned_deps.len() + orphaned_links.len();
    Ok(OrphansResult {
        orphaned_deps: scan.orphaned_deps,
        orphaned_links,
        total,
    })
}

fn sort_dependency_refs(mut refs: Vec<DependencyRef>) -> Vec<DependencyRef> {
    refs.sort_by(|a, b| {
        if a.id == b.id {
            return dep_type_to_string(a.dep_type).cmp(dep_type_to_string(b.dep_type));
        }
        a.id.cmp(&b.id)
    });
    refs
}

fn matches_dep_type_filter(
    state: &crate::types::State,
    dependents_by_blocker: &HashMap<String, Vec<crate::domain::dep_tree::DependentEdge>>,
    task_id: &str,
    dep_type: DependencyType,
    direction: DepDirectionFilter,
) -> bool {
    let has_out = normalize_dependency_edges(state.deps.get(task_id))
        .into_iter()
        .any(|edge| edge.dep_type == dep_type);
    let has_in = dependents_by_blocker
        .get(task_id)
        .map(|edges| edges.iter().any(|edge| edge.dep_type == dep_type))
        .unwrap_or(false);
    match direction {
        DepDirectionFilter::Out => has_out,
        DepDirectionFilter::In => has_in,
        DepDirectionFilter::Any => has_out || has_in,
    }
}

fn unique_ids(edges: &[DependencyRef]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for edge in edges {
        if seen.insert(edge.id.clone()) {
            ids.push(edge.id.clone());
        }
    }
    ids
}

fn relation_type_to_string(rel_type: RelationType) -> &'static str {
    match rel_type {
        RelationType::RelatesTo => "relates_to",
        RelationType::RepliesTo => "replies_to",
        RelationType::Duplicates => "duplicates",
        RelationType::Supersedes => "supersedes",
    }
}

fn dep_type_to_string(dep_type: DependencyType) -> &'static str {
    match dep_type {
        DependencyType::Blocks => "blocks",
        DependencyType::StartsAfter => "starts_after",
    }
}

fn event_type_to_string(event_type: EventType) -> &'static str {
    match event_type {
        EventType::TaskCreated => "task.created",
        EventType::TaskUpdated => "task.updated",
        EventType::TaskStatusSet => "task.status_set",
        EventType::TaskClaimed => "task.claimed",
        EventType::TaskNoted => "task.noted",
        EventType::TaskSpecAttached => "task.spec_attached",
        EventType::TaskSuperseded => "task.superseded",
        EventType::DepAdded => "dep.added",
        EventType::DepRemoved => "dep.removed",
        EventType::LinkAdded => "link.added",
        EventType::LinkRemoved => "link.removed",
    }
}
