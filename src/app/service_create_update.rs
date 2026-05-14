use crate::app::service_types::{CreateBatchInput, CreateInput, ServiceContext, UpdateInput};
use crate::app::service_utils::{
    must_resolve_existing, must_task, normalize_duplicate_title, unique_root_id,
};
use crate::app::storage::{
    append_events, load_projected_state, persist_projection, with_write_lock,
};
use crate::domain::alias::allocate_alias;
use crate::domain::events::make_event;
use crate::domain::ids::{RootIdAllocator, is_valid_root_id, next_child_id};
use crate::domain::projector::apply_events;
use crate::domain::similarity::{
    DEFAULT_SIMILARITY_LIMIT, DEFAULT_SIMILARITY_MIN_SCORE, blocking_status,
    find_similar_candidates, is_blocking_duplicate, is_blocking_title_pair,
};
use crate::errors::TsqError;
use crate::types::{EventRecord, EventType, PlanningState, State, Task, TaskStatus};
use serde_json::{Map, Value};

pub fn create(ctx: &ServiceContext, input: &CreateInput) -> Result<Task, TsqError> {
    if input.explicit_id.is_some() && input.parent.is_some() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --id with --parent",
            1,
        ));
    }
    if input.ensure && input.explicit_id.is_some() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --ensure with --id",
            1,
        ));
    }
    if input.ensure && input.force {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --ensure with --force",
            1,
        ));
    }
    if input.description.is_some() && input.body_file.is_some() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --description with --body-file",
            1,
        ));
    }

    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;

        let (id, parent_id) = if let Some(explicit_id) = input.explicit_id.as_ref() {
            if !is_valid_root_id(explicit_id) {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "explicit --id must match tsq-<number> or legacy tsq-<8 crockford base32 chars>",
                    1,
                ));
            }
            if loaded.state.tasks.contains_key(explicit_id) {
                return Err(TsqError::new(
                    "ID_COLLISION",
                    format!("task already exists: {}", explicit_id),
                    1,
                ));
            }
            // Duplicate gate: explicit --id still checks for similar existing tasks.
            check_duplicate_gate(
                &loaded.state,
                &input.title,
                input.force,
                input.skip_duplicate_check,
            )?;
            (explicit_id.clone(), None)
        } else {
            let parent_id = input
                .parent
                .as_ref()
                .map(|raw| must_resolve_existing(&loaded.state, raw, input.exact_id))
                .transpose()?;
            if input.ensure
                && let Some(existing) = find_existing_by_parent_and_title(
                    &loaded.state,
                    parent_id.as_deref(),
                    &input.title,
                )
            {
                return Ok(existing);
            }
            check_duplicate_gate(
                &loaded.state,
                &input.title,
                input.force,
                input.skip_duplicate_check,
            )?;
            let id = if let Some(parent) = parent_id.as_ref() {
                next_child_id(&loaded.state, parent)
            } else {
                unique_root_id(&loaded.state, &input.title)?
            };
            (id, parent_id)
        };

        let description = if input.body_file.is_some() {
            input.body_file.clone()
        } else {
            input.description.clone()
        };
        let discovered_from = input
            .discovered_from
            .as_ref()
            .map(|raw| must_resolve_existing(&loaded.state, raw, input.exact_id))
            .transpose()?;
        let ts = ctx.now.as_ref()();
        let alias = allocate_alias(&loaded.state, &input.title)?;

        let duplicate_candidates = if input.force {
            let candidates = find_similar_candidates(
                loaded.state.tasks.values(),
                &input.title,
                DEFAULT_SIMILARITY_MIN_SCORE,
                DEFAULT_SIMILARITY_LIMIT,
            );
            if candidates.is_empty() {
                None
            } else {
                Some(Value::Array(
                    candidates.iter().map(duplicate_candidate_json).collect(),
                ))
            }
        } else {
            None
        };

        let mut payload = serde_json::json!({
          "id": id,
          "title": input.title,
          "alias": alias,
          "description": description,
          "external_ref": input.external_ref,
          "discovered_from": discovered_from,
          "kind": input.kind,
          "priority": input.priority,
          "status": TaskStatus::Open,
          "parent_id": parent_id,
          "planning_state": input.planning_state.unwrap_or(PlanningState::NeedsPlanning),
        });
        if let Some(candidates) = duplicate_candidates {
            payload["duplicate_candidates"] = candidates;
        }

        let event = make_event(
            &ctx.actor,
            &ts,
            EventType::TaskCreated,
            &id,
            payload_map(payload),
        );

        let mut next_state = apply_events(&loaded.state, std::slice::from_ref(&event))?;
        append_events(&ctx.repo_root, &[event])?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.event_count + 1,
            None,
        )?;
        must_task(&next_state, &id)
    })
}

pub fn update(ctx: &ServiceContext, input: &UpdateInput) -> Result<Task, TsqError> {
    if input.description.is_some() && input.clear_description {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --description with --clear-description",
            1,
        ));
    }
    if input.external_ref.is_some() && input.clear_external_ref {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --external-ref with --clear-external-ref",
            1,
        ));
    }
    if input.discovered_from.is_some() && input.clear_discovered_from {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --discovered-from with --clear-discovered-from",
            1,
        ));
    }

    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;
        let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;
        let existing = must_task(&loaded.state, &id)?;
        let mut patch = Map::new();

        if let Some(title) = input.title.as_ref() {
            patch.insert("title".to_string(), Value::String(title.clone()));
        }
        if let Some(priority) = input.priority {
            patch.insert("priority".to_string(), serde_json::json!(priority));
        }
        if let Some(description) = input.description.as_ref() {
            patch.insert(
                "description".to_string(),
                Value::String(description.clone()),
            );
        }
        if input.clear_description {
            patch.insert("clear_description".to_string(), Value::Bool(true));
        }
        if let Some(external_ref) = input.external_ref.as_ref() {
            patch.insert(
                "external_ref".to_string(),
                Value::String(external_ref.clone()),
            );
        }
        if input.clear_external_ref {
            patch.insert("clear_external_ref".to_string(), Value::Bool(true));
        }
        if let Some(planning_state) = input.planning_state {
            patch.insert(
                "planning_state".to_string(),
                serde_json::json!(planning_state),
            );
        }
        if let Some(assignee) = input.assignee.as_ref() {
            patch.insert("assignee".to_string(), Value::String(assignee.clone()));
        }
        if let Some(discovered_from_raw) = input.discovered_from.as_ref() {
            let discovered_from =
                must_resolve_existing(&loaded.state, discovered_from_raw, input.exact_id)?;
            patch.insert(
                "discovered_from".to_string(),
                Value::String(discovered_from),
            );
        }
        if input.clear_discovered_from {
            patch.insert("clear_discovered_from".to_string(), Value::Bool(true));
        }

        let has_field_patch = !patch.is_empty();
        let has_status_change = input.status.is_some();
        if !has_field_patch && !has_status_change {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "no update fields provided",
                1,
            ));
        }

        if existing.status == TaskStatus::Canceled && input.status == Some(TaskStatus::InProgress) {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "cannot move canceled task to in_progress",
                1,
            ));
        }

        let mut events: Vec<EventRecord> = Vec::new();
        if has_field_patch {
            events.push(make_event(
                &ctx.actor,
                &ctx.now.as_ref()(),
                EventType::TaskUpdated,
                &id,
                patch,
            ));
        }
        if let Some(status) = input.status {
            let ts = ctx.now.as_ref()();
            let closed_at = if status == TaskStatus::Closed {
                Some(ts.clone())
            } else {
                None
            };
            events.push(make_event(
                &ctx.actor,
                &ts,
                EventType::TaskStatusSet,
                &id,
                payload_map(serde_json::json!({
                  "status": status,
                  "closed_at": closed_at,
                })),
            ));
        }

        let mut next_state = apply_events(&loaded.state, &events)?;
        append_events(&ctx.repo_root, &events)?;
        persist_projection(
            &ctx.repo_root,
            &mut next_state,
            loaded.event_count + events.len(),
            None,
        )?;
        must_task(&next_state, &id)
    })
}

fn payload_map(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn duplicate_candidate_json(c: &crate::domain::similarity::SimilarTaskCandidate) -> Value {
    serde_json::json!({
        "id": c.task.id,
        "alias": c.task.alias,
        "title": c.task.title,
        "status": c.task.status,
        "score": c.score,
        "reason": c.reason,
    })
}

fn check_duplicate_gate(
    state: &State,
    title: &str,
    force: bool,
    skip_duplicate_check: bool,
) -> Result<(), TsqError> {
    if force || skip_duplicate_check {
        return Ok(());
    }

    let candidates: Vec<_> = state
        .tasks
        .values()
        .filter(|task| blocking_status(task.status))
        .filter_map(|task| is_blocking_duplicate(title, task))
        .collect();
    if candidates.is_empty() {
        return Ok(());
    }

    let details_candidates: Vec<Value> = candidates.iter().map(duplicate_candidate_json).collect();
    Err(TsqError::new(
        "DUPLICATE_TASK_CANDIDATE",
        "similar task already exists; use --force to create anyway",
        1,
    )
    .with_details(serde_json::json!({
        "input_title": title,
        "candidates": details_candidates
    })))
}

/// Atomic batch create: acquires one write lock, validates all items
/// (incoming pairwise + existing duplicate checks with parent-aware ensure
/// logic), then appends all events together.
///
/// Returns the created (or ensure-reused) tasks in input order.
pub fn create_batch(ctx: &ServiceContext, input: &CreateBatchInput) -> Result<Vec<Task>, TsqError> {
    if input.ensure && input.force {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --ensure with --force",
            1,
        ));
    }

    with_write_lock(&ctx.repo_root, || {
        let loaded = load_projected_state(&ctx.repo_root)?;

        // Resolve CLI --parent once.
        let cli_parent_id: Option<String> = input
            .parent
            .as_ref()
            .map(|raw| must_resolve_existing(&loaded.state, raw, input.exact_id))
            .transpose()?;

        // Phase 1: Resolve planned parents and determine which items are
        // ensure-reusable vs new.  For from-file, parent depends on depth
        // stacking; for positional, all share cli_parent_id.
        let planned = resolve_batch_plan(&loaded.state, input, cli_parent_id.as_deref())?;

        // Phase 2: Incoming pairwise duplicate check.
        if !input.force {
            check_incoming_duplicates(&planned, input.ensure)?;
        }

        // Phase 3: Existing-task duplicate check.
        if !input.force {
            check_existing_duplicates(&loaded.state, &planned, input.ensure)?;
        }

        // Phase 4: Generate events for all new items.
        // For from-file batches, maintain a depth→ID stack to resolve parents
        // of items whose parent was BatchNew at planning time.
        let mut events: Vec<EventRecord> = Vec::new();
        let mut result_tasks: Vec<Task> = Vec::with_capacity(planned.len());
        let mut working_state = loaded.state.clone();
        let mut parent_stack: Vec<String> = Vec::new(); // depth → created/reused ID
        let mut root_id_allocator: Option<RootIdAllocator> = None;

        // Track tasks created in this batch by (normalized_title, parent_id)
        // so ensure can reuse earlier batch-created tasks for exact duplicates.
        let mut batch_created: std::collections::HashMap<(String, Option<String>), Task> =
            std::collections::HashMap::new();

        let discovered_from = input
            .discovered_from
            .as_ref()
            .map(|raw| must_resolve_existing(&loaded.state, raw, input.exact_id))
            .transpose()?;

        for item in &planned {
            let depth = item.depth();

            match item {
                PlannedItem::Reuse(task, _) => {
                    parent_stack.truncate(depth);
                    parent_stack.push(task.id.clone());
                    result_tasks.push(task.as_ref().clone());
                }
                PlannedItem::New {
                    title,
                    parent_id: planned_parent_id,
                    ..
                } => {
                    // Resolve parent: use planned_parent_id if known, otherwise
                    // look up from parent_stack (for BatchNew parents).
                    let parent_id = if planned_parent_id.is_some() {
                        planned_parent_id.clone()
                    } else if input.from_file && depth > 0 {
                        parent_stack.get(depth - 1).cloned()
                    } else {
                        None
                    };

                    // Ensure dedup: reuse earlier batch-created task with same
                    // normalized title and parent instead of creating a duplicate.
                    if input.ensure {
                        let key = (normalize_duplicate_title(title), parent_id.clone());
                        if let Some(existing) = batch_created.get(&key) {
                            parent_stack.truncate(depth);
                            parent_stack.push(existing.id.clone());
                            result_tasks.push(existing.clone());
                            continue;
                        }
                    }

                    let id = if let Some(parent) = parent_id.as_ref() {
                        next_child_id(&working_state, parent)
                    } else {
                        if root_id_allocator.is_none() {
                            root_id_allocator = Some(RootIdAllocator::new(&loaded.state)?);
                        }
                        root_id_allocator
                            .as_mut()
                            .expect("root id allocator initialized")
                            .next_id()?
                    };

                    let description = if input.body_file.is_some() {
                        input.body_file.clone()
                    } else {
                        input.description.clone()
                    };
                    let ts = ctx.now.as_ref()();
                    let alias = allocate_alias(&working_state, title)?;

                    let duplicate_candidates = if input.force {
                        let candidates = find_similar_candidates(
                            working_state.tasks.values(),
                            title,
                            DEFAULT_SIMILARITY_MIN_SCORE,
                            DEFAULT_SIMILARITY_LIMIT,
                        );
                        if candidates.is_empty() {
                            None
                        } else {
                            Some(Value::Array(
                                candidates.iter().map(duplicate_candidate_json).collect(),
                            ))
                        }
                    } else {
                        None
                    };

                    let mut payload = serde_json::json!({
                        "id": id,
                        "title": title,
                        "alias": alias,
                        "description": description,
                        "external_ref": input.external_ref,
                        "discovered_from": discovered_from,
                        "kind": input.kind,
                        "priority": input.priority,
                        "status": TaskStatus::Open,
                        "parent_id": parent_id,
                        "planning_state": input.planning_state.unwrap_or(PlanningState::NeedsPlanning),
                    });
                    if let Some(candidates) = duplicate_candidates {
                        payload["duplicate_candidates"] = candidates;
                    }

                    let event = make_event(
                        &ctx.actor,
                        &ts,
                        EventType::TaskCreated,
                        &id,
                        payload_map(payload),
                    );

                    working_state = apply_events(&working_state, std::slice::from_ref(&event))?;
                    events.push(event);
                    let task = must_task(&working_state, &id)?;
                    if input.ensure {
                        let key = (normalize_duplicate_title(title), parent_id.clone());
                        batch_created.insert(key, task.clone());
                    }
                    parent_stack.truncate(depth);
                    parent_stack.push(task.id.clone());
                    result_tasks.push(task);
                }
            }
        }

        // Phase 5: Persist all events atomically.
        if !events.is_empty() {
            append_events(&ctx.repo_root, &events)?;
            persist_projection(
                &ctx.repo_root,
                &mut working_state,
                loaded.event_count + events.len(),
                None,
            )?;
        }

        // In ensure mode, result_tasks may contain the same task.id multiple
        // times; retain one copy so idempotent duplicate input yields unique output.
        if input.ensure {
            let mut seen = std::collections::HashSet::new();
            result_tasks.retain(|task| seen.insert(task.id.clone()));
        }

        Ok(result_tasks)
    })
}

/// Planned parent identity for a batch item.
#[derive(Clone, Debug)]
enum PlannedParentId {
    /// Resolved to an existing parent (or root if None).
    Known(Option<String>),
    /// Parent is a new task created earlier in this batch.
    /// Cannot be resolved to an existing task.
    BatchNew,
}

/// A resolved batch item: either an existing task to reuse (ensure) or a new task.
#[derive(Clone, Debug)]
enum PlannedItem {
    Reuse(Box<Task>, usize),
    New {
        title: String,
        parent_id: Option<String>,
        planned_parent: PlannedParentId,
        marker: Option<usize>,
        depth: usize,
    },
}

impl PlannedItem {
    fn title(&self) -> &str {
        match self {
            PlannedItem::Reuse(task, _) => &task.title,
            PlannedItem::New { title, .. } => title,
        }
    }

    fn marker(&self) -> Option<usize> {
        match self {
            PlannedItem::Reuse(..) => None,
            PlannedItem::New { marker, .. } => *marker,
        }
    }

    fn planned_parent(&self) -> PlannedParentId {
        match self {
            PlannedItem::Reuse(task, _) => PlannedParentId::Known(task.parent_id.clone()),
            PlannedItem::New { planned_parent, .. } => planned_parent.clone(),
        }
    }

    fn depth(&self) -> usize {
        match self {
            PlannedItem::Reuse(_, depth) => *depth,
            PlannedItem::New { depth, .. } => *depth,
        }
    }

    fn is_reuse(&self) -> bool {
        matches!(self, PlannedItem::Reuse(..))
    }
}

/// Build the batch plan: for each input item, determine if it's an ensure-reuse
/// or a new task, and resolve its parent identity.
fn resolve_batch_plan(
    state: &State,
    input: &CreateBatchInput,
    cli_parent_id: Option<&str>,
) -> Result<Vec<PlannedItem>, TsqError> {
    let mut planned: Vec<PlannedItem> = Vec::with_capacity(input.items.len());

    if input.from_file {
        // From-file: depth-based parent stacking.
        // identity_stack[depth] = Some(id) when item at that depth resolved to
        //                         an existing task (ensure reuse or known parent)
        //                       = None when the item is brand new
        let mut identity_stack: Vec<Option<String>> = Vec::new();

        for item in input.items.iter() {
            let (parent_id, planned_parent) = if item.depth == 0 {
                (
                    cli_parent_id.map(|s| s.to_string()),
                    PlannedParentId::Known(cli_parent_id.map(|s| s.to_string())),
                )
            } else {
                let parent_depth = item.depth - 1;
                if parent_depth >= identity_stack.len() {
                    return Err(TsqError::new(
                        "VALIDATION_ERROR",
                        format!(
                            "line {} has no parent at depth {}",
                            item.marker.unwrap_or(0),
                            parent_depth
                        ),
                        1,
                    ));
                }
                match &identity_stack[parent_depth] {
                    Some(id) => (Some(id.clone()), PlannedParentId::Known(Some(id.clone()))),
                    None => {
                        // Parent is a new task in this batch.
                        (None, PlannedParentId::BatchNew)
                    }
                }
            };

            // Try ensure-reuse if parent is known.
            let reused = if input.ensure {
                match &planned_parent {
                    PlannedParentId::Known(pid) => {
                        find_existing_by_parent_and_title(state, pid.as_deref(), &item.title)
                    }
                    PlannedParentId::BatchNew => None,
                }
            } else {
                None
            };

            let identity: Option<String> = reused.as_ref().map(|t| t.id.clone());

            if let Some(task) = reused {
                planned.push(PlannedItem::Reuse(Box::new(task), item.depth));
            } else {
                planned.push(PlannedItem::New {
                    title: item.title.clone(),
                    parent_id,
                    planned_parent,
                    marker: item.marker,
                    depth: item.depth,
                });
            }

            identity_stack.truncate(item.depth);
            identity_stack.push(identity);
        }
    } else {
        // Positional titles: all share cli_parent_id.
        let pp = PlannedParentId::Known(cli_parent_id.map(|s| s.to_string()));
        for item in &input.items {
            let reused = if input.ensure {
                find_existing_by_parent_and_title(state, cli_parent_id, &item.title)
            } else {
                None
            };
            if let Some(task) = reused {
                planned.push(PlannedItem::Reuse(Box::new(task), 0));
            } else {
                planned.push(PlannedItem::New {
                    title: item.title.clone(),
                    parent_id: cli_parent_id.map(|s| s.to_string()),
                    planned_parent: pp.clone(),
                    marker: item.marker,
                    depth: 0,
                });
            }
        }
    }

    Ok(planned)
}

/// Check incoming items against each other for similarity.
/// With --ensure, exact-title pairs are always safe: same-parent pairs will be
/// deduplicated by ensure, and different-parent pairs are legitimately distinct.
fn check_incoming_duplicates(planned: &[PlannedItem], ensure: bool) -> Result<(), TsqError> {
    for (i, item_a) in planned.iter().enumerate() {
        if item_a.is_reuse() {
            continue;
        }
        for item_b in planned.iter().skip(i + 1) {
            if item_b.is_reuse() {
                continue;
            }
            if let Some((score, reason)) = is_blocking_title_pair(item_a.title(), item_b.title()) {
                // With --ensure, exact normalized matches are safe: ensure
                // reuses the first occurrence for same-parent, and different
                // parents are legitimately distinct tasks.
                if ensure && reason == "normalized_title_exact" {
                    continue;
                }
                return Err(TsqError::new(
                    "DUPLICATE_TASK_CANDIDATE",
                    "duplicate task title in create input; use --force to create anyway",
                    1,
                )
                .with_details(serde_json::json!({
                    "input_title": item_b.title(),
                    "input_marker": item_b.marker(),
                    "candidates": [{
                        "title": item_a.title(),
                        "marker": item_a.marker(),
                        "score": score,
                        "reason": reason,
                    }]
                })));
            }
        }
    }
    Ok(())
}

/// Check new items against existing tasks for duplicates.
/// With --ensure, exempt exact-title matches only when the existing task's
/// parent matches the item's planned parent (these would have been caught by
/// ensure-reuse in resolve_batch_plan already, but we need to handle the case
/// where the item's parent is BatchNew — then we cannot match any existing task).
fn check_existing_duplicates(
    state: &State,
    planned: &[PlannedItem],
    ensure: bool,
) -> Result<(), TsqError> {
    for item in planned {
        if item.is_reuse() {
            continue;
        }
        let title = item.title();
        let candidates: Vec<_> = state
            .tasks
            .values()
            .filter(|task| blocking_status(task.status))
            .filter_map(|task| is_blocking_duplicate(title, task))
            .filter(|candidate| {
                if !ensure {
                    return true;
                }
                if candidate.reason != "normalized_title_exact" {
                    return true;
                }
                // Exact match: only block if parent differs from planned.
                match item.planned_parent() {
                    PlannedParentId::Known(parent_id) => {
                        candidate.task.parent_id.as_deref() != parent_id.as_deref()
                    }
                    PlannedParentId::BatchNew => {
                        // Parent is new in this batch — no existing task can match.
                        true
                    }
                }
            })
            .collect();

        if !candidates.is_empty() {
            let details_candidates: Vec<Value> =
                candidates.iter().map(duplicate_candidate_json).collect();
            return Err(TsqError::new(
                "DUPLICATE_TASK_CANDIDATE",
                "similar task already exists; use --force to create anyway",
                1,
            )
            .with_details(serde_json::json!({
                "input_title": title,
                "input_marker": item.marker(),
                "candidates": details_candidates
            })));
        }
    }
    Ok(())
}

fn find_existing_by_parent_and_title(
    state: &State,
    parent_id: Option<&str>,
    title: &str,
) -> Option<Task> {
    let normalized_title = normalize_duplicate_title(title);
    let mut matches: Vec<&Task> = state
        .tasks
        .values()
        .filter(|task| task.parent_id.as_deref() == parent_id)
        .filter(|task| normalize_duplicate_title(&task.title) == normalized_title)
        .collect();
    matches.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    matches.first().map(|task| (*task).clone())
}
