use crate::app::service_query::ShowResult;
use crate::app::service_types::{HistoryResult, MergeResult, OrphansResult};
use crate::cli::style;
use crate::cli::terminal::{Density, resolve_density, resolve_width};
use crate::domain::dep_tree::DepTreeNode;
use crate::types::{RepairResult, Task, TaskNote, TaskStatus, TaskTreeNode};
use std::collections::HashMap;

pub struct TreeRenderOptions {
    pub width: Option<usize>,
}

pub fn print_task_list(tasks: &[Task]) {
    if tasks.is_empty() {
        println!("{}", style::muted("no tasks"));
        return;
    }

    let header = ["ID", "P", "KIND", "STATUS", "ASSIGNEE", "TITLE"];
    let rows: Vec<Vec<String>> = tasks
        .iter()
        .map(|task| {
            vec![
                task.id.clone(),
                task.priority.to_string(),
                task_kind_to_string(task.kind).to_string(),
                status_to_string(task.status).to_string(),
                task.assignee.clone().unwrap_or_else(|| "-".to_string()),
                task.title.clone(),
            ]
        })
        .collect();

    let mut widths: Vec<usize> = header.iter().map(|value| value.len()).collect();
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            if cell.len() > widths[index] {
                widths[index] = cell.len();
            }
        }
    }

    println!(
        "{}",
        style::heading(
            header
                .iter()
                .enumerate()
                .map(|(index, cell)| format!("{:width$}", cell, width = widths[index]))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end(),
        )
    );

    for row in &rows {
        let formatted_cells = row
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                let padded = format!("{:width$}", cell, width = widths[index]);
                if index == 0 {
                    style::task_id(&padded)
                } else if index == 3 {
                    style::status(
                        &padded,
                        parse_status_label(cell.as_str()).unwrap_or(TaskStatus::Open),
                    )
                } else {
                    padded
                }
            })
            .collect::<Vec<_>>();
        println!("{}", formatted_cells.join("  ").trim_end());
    }
}

pub fn print_task(task: &Task) {
    println!("{} {}", style::task_id(&task.id), task.title);
    println!(
        "{}={} {}={} {}={}",
        style::key("kind"),
        task_kind_to_string(task.kind),
        style::key("status"),
        status_to_string(task.status),
        style::key("priority"),
        task.priority
    );
    if let Some(planning_state) = task.planning_state {
        println!(
            "{}={}",
            style::key("planning"),
            planning_state_to_string(planning_state)
        );
    }
    if let Some(assignee) = &task.assignee {
        println!("{}={}", style::key("assignee"), assignee);
    }
    if let Some(external_ref) = &task.external_ref {
        println!("{}={}", style::key("external_ref"), external_ref);
    }
    if let Some(discovered_from) = &task.discovered_from {
        println!("{}={}", style::key("discovered_from"), discovered_from);
    }
    if let Some(parent) = &task.parent_id {
        println!("{}={}", style::key("parent"), parent);
    }
    if let Some(superseded_by) = &task.superseded_by {
        println!("{}={}", style::key("superseded_by"), superseded_by);
    }
    if let Some(duplicate_of) = &task.duplicate_of {
        println!("{}={}", style::key("duplicate_of"), duplicate_of);
    }
    if let Some(description) = &task.description {
        println!("{}={}", style::key("description"), description);
    }
    println!("{}={}", style::key("notes"), task.notes.len());
    if let (Some(spec_path), Some(spec_fingerprint)) = (&task.spec_path, &task.spec_fingerprint) {
        let by = task
            .spec_attached_by
            .as_ref()
            .map(|value| format!(" by={}", value))
            .unwrap_or_default();
        let at = task
            .spec_attached_at
            .as_ref()
            .map(|value| format!(" at={}", value))
            .unwrap_or_default();
        println!(
            "{}={} {}={}{}{}",
            style::key("spec"),
            spec_path,
            style::key("sha256"),
            spec_fingerprint,
            by,
            at
        );
    }
}

pub fn print_show_result(data: &ShowResult) {
    print_task(&data.task);
    if !data.blocker_edges.is_empty() {
        let blockers = data
            .blocker_edges
            .iter()
            .map(|edge| format!("{}({})", edge.id, dep_type_to_string(edge.dep_type)))
            .collect::<Vec<_>>();
        println!("{}={}", style::key("blockers"), blockers.join(","));
    } else if !data.blockers.is_empty() {
        println!("{}={}", style::key("blockers"), data.blockers.join(","));
    }

    if !data.dependent_edges.is_empty() {
        let dependents = data
            .dependent_edges
            .iter()
            .map(|edge| format!("{}({})", edge.id, dep_type_to_string(edge.dep_type)))
            .collect::<Vec<_>>();
        println!("{}={}", style::key("dependents"), dependents.join(","));
    } else if !data.dependents.is_empty() {
        println!("{}={}", style::key("dependents"), data.dependents.join(","));
    }

    println!("{}={}", style::key("ready"), data.ready);
    if !data.links.is_empty() {
        println!(
            "{}={}",
            style::key("links"),
            serde_json::to_string(&data.links).unwrap_or_else(|_| "{}".to_string())
        );
    }
    if !data.history.is_empty() {
        println!("{}={}", style::key("history_events"), data.history.len());
    }
}

pub fn print_task_tree(nodes: &[TaskTreeNode]) {
    for line in render_task_tree(nodes, TreeRenderOptions { width: None }) {
        println!("{}", line);
    }
}

pub fn render_task_tree(nodes: &[TaskTreeNode], options: TreeRenderOptions) -> Vec<String> {
    if nodes.is_empty() {
        return vec![style::muted("no tasks")];
    }

    let width = resolve_width(options.width);
    let density = resolve_density(width);
    let mut lines = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        render_tree_node(
            &mut lines,
            node,
            "",
            index + 1 == nodes.len(),
            true,
            density,
            width,
        );
    }

    let totals = summarize_tree(nodes);
    lines.push(format!(
        "total={} open={} in_progress={} blocked={} deferred={} closed={} canceled={}",
        totals.get("total").copied().unwrap_or(0),
        totals.get("open").copied().unwrap_or(0),
        totals.get("in_progress").copied().unwrap_or(0),
        totals.get("blocked").copied().unwrap_or(0),
        totals.get("deferred").copied().unwrap_or(0),
        totals.get("closed").copied().unwrap_or(0),
        totals.get("canceled").copied().unwrap_or(0)
    ));
    lines
}

fn render_tree_node(
    lines: &mut Vec<String>,
    node: &TaskTreeNode,
    prefix: &str,
    is_last: bool,
    root: bool,
    density: Density,
    width: usize,
) {
    let connector = if root {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };
    let line_prefix = format!("{}{}", prefix, connector);
    let child_prefix = if root {
        prefix.to_string()
    } else if is_last {
        format!("{}    ", prefix)
    } else {
        format!("{}│   ", prefix)
    };
    let meta_prefix = if root {
        if node.children.is_empty() {
            "    ".to_string()
        } else {
            "│   ".to_string()
        }
    } else {
        child_prefix.clone()
    };

    let status = format_status(node.task.status);
    let status_text = format_status_text(node.task.status);
    let flow = format_flow(node);
    let task_id = style::task_id(&node.task.id);
    let mut primary_parts = vec![
        status,
        task_id,
        if density == Density::Narrow {
            let max_title_width = compute_title_width(
                width,
                line_prefix.len(),
                status_text.len(),
                node.task.id.len(),
            );
            truncate_with_ellipsis(&node.task.title, max_title_width)
        } else {
            node.task.title.clone()
        },
    ];
    if density != Density::Narrow {
        primary_parts.push(format_meta_badge(&node.task));
    }
    if density == Density::Wide
        && let Some(flow) = &flow
    {
        primary_parts.push(flow.clone());
    }

    lines.push(format!(
        "{}{}",
        style::tree_prefix(&line_prefix),
        primary_parts.join(" ")
    ));
    if density == Density::Medium
        && let Some(flow) = &flow
    {
        lines.push(format!("{}{}", style::tree_prefix(&meta_prefix), flow));
    }
    if density == Density::Narrow {
        lines.push(format!(
            "{}{}",
            style::tree_prefix(&meta_prefix),
            format_meta_badge(&node.task)
        ));
        if let Some(flow) = &flow {
            lines.push(format!("{}{}", style::tree_prefix(&meta_prefix), flow));
        }
    }

    for (index, child) in node.children.iter().enumerate() {
        render_tree_node(
            lines,
            child,
            &child_prefix,
            index + 1 == node.children.len(),
            false,
            density,
            width,
        );
    }
}

pub fn print_repair_result(result: &RepairResult) {
    println!(
        "mode={}",
        if result.applied {
            "applied"
        } else {
            "dry-run (use --fix to apply)"
        }
    );
    println!(
        "orphaned_deps={}{}",
        result.plan.orphaned_deps.len(),
        if result.applied { " (removed)" } else { "" }
    );
    for dep in &result.plan.orphaned_deps {
        println!(
            "  {} -> {} ({})",
            dep.child,
            dep.blocker,
            dep_type_to_string(dep.dep_type)
        );
    }
    println!(
        "orphaned_links={}{}",
        result.plan.orphaned_links.len(),
        if result.applied { " (removed)" } else { "" }
    );
    for link in &result.plan.orphaned_links {
        println!(
            "  {} -[{}]-> {}",
            link.src,
            relation_type_to_string(link.rel_type),
            link.dst
        );
    }
    println!(
        "stale_temps={}{}",
        result.plan.stale_temps.len(),
        if result.applied { " (deleted)" } else { "" }
    );
    println!("stale_lock={}", result.plan.stale_lock);
    println!(
        "old_snapshots={}{}",
        result.plan.old_snapshots.len(),
        if result.applied && !result.plan.old_snapshots.is_empty() {
            " (pruned, kept last 5)"
        } else {
            ""
        }
    );
    if result.applied {
        println!("events_appended={}", result.events_appended);
        println!("files_removed={}", result.files_removed);
    }
}

pub fn print_merge_result(result: &MergeResult) {
    if result.dry_run {
        println!(
            "{}={}",
            style::key("mode"),
            style::warning("dry-run (use without --dry-run to apply)")
        );
    }
    println!(
        "{}={} \"{}\" [{}]",
        style::key("target"),
        style::task_id(&result.target.id),
        result.target.title,
        result.target.status
    );
    if let Some(summary) = &result.plan_summary {
        println!(
            "plan=requested:{} merged:{} skipped:{} events:{}",
            summary.requested_sources,
            summary.merged_sources,
            summary.skipped_sources,
            summary.planned_events
        );
    }
    println!("merged={}", result.merged.len());
    for merged in &result.merged {
        println!("  {} -> {}", merged.id, merged.status);
    }
    if let Some(projected) = &result.projected {
        println!(
            "projected_target={} [{}] planning={}",
            projected.target.id,
            status_to_string(projected.target.status),
            projected
                .target
                .planning_state
                .map(planning_state_to_string)
                .unwrap_or("needs_planning")
        );
        for source in &projected.sources {
            println!(
                "  projected_source={} [{}] duplicate_of={}",
                source.id,
                status_to_string(source.status),
                source.duplicate_of.as_deref().unwrap_or("-")
            );
        }
    }
    for warning in &result.warnings {
        println!("{}: {}", style::warning("warning"), warning);
    }
}

pub fn format_meta_badge(task: &Task) -> String {
    let assignee = task
        .assignee
        .as_ref()
        .map(|value| format!(" @{}", value))
        .unwrap_or_default();
    style::meta(&format!("[p{}{}]", task.priority, assignee))
}

fn format_flow(node: &TaskTreeNode) -> Option<String> {
    let mut flow = Vec::new();
    if let Some(blocker_edges) = &node.blocker_edges {
        if !blocker_edges.is_empty() {
            let values = blocker_edges
                .iter()
                .map(|edge| format!("{}:{}", edge.id, dep_type_to_string(edge.dep_type)))
                .collect::<Vec<_>>()
                .join(",");
            flow.push(format!("blocks-on: {}", values));
        }
    } else if !node.blockers.is_empty() {
        flow.push(format!("blocks-on: {}", node.blockers.join(",")));
    }

    if let Some(dependent_edges) = &node.dependent_edges {
        if !dependent_edges.is_empty() {
            let values = dependent_edges
                .iter()
                .map(|edge| format!("{}:{}", edge.id, dep_type_to_string(edge.dep_type)))
                .collect::<Vec<_>>()
                .join(",");
            flow.push(format!("unblocks: {}", values));
        }
    } else if !node.dependents.is_empty() {
        flow.push(format!("unblocks: {}", node.dependents.join(",")));
    }

    if flow.is_empty() {
        None
    } else {
        Some(style::flow(&format!("{{{}}}", flow.join(" | "))))
    }
}

fn compute_title_width(
    width: usize,
    prefix_length: usize,
    status_length: usize,
    task_id_length: usize,
) -> usize {
    let fixed_length = prefix_length + status_length + 1 + task_id_length + 1;
    let remaining = width.saturating_sub(fixed_length);
    remaining.max(12)
}

pub fn truncate_with_ellipsis(value: &str, max_length: usize) -> String {
    if value.chars().count() <= max_length {
        return value.to_string();
    }
    if max_length <= 3 {
        return value.chars().take(max_length).collect();
    }
    let truncated: String = value.chars().take(max_length - 3).collect();
    format!("{}...", truncated)
}

pub fn format_status_text(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Open => "○ open",
        TaskStatus::InProgress => "◐ in_progress",
        TaskStatus::Blocked => "● blocked",
        TaskStatus::Closed => "✓ closed",
        TaskStatus::Canceled => "✕ canceled",
        TaskStatus::Deferred => "◇ deferred",
    }
}

pub fn format_status(status: TaskStatus) -> String {
    style::status(format_status_text(status), status)
}

fn summarize_tree(nodes: &[TaskTreeNode]) -> HashMap<&'static str, usize> {
    let mut summary = HashMap::new();
    summary.insert("total", 0);
    summary.insert("open", 0);
    summary.insert("in_progress", 0);
    summary.insert("blocked", 0);
    summary.insert("deferred", 0);
    summary.insert("closed", 0);
    summary.insert("canceled", 0);

    fn visit(node: &TaskTreeNode, summary: &mut HashMap<&'static str, usize>) {
        *summary.get_mut("total").expect("total exists") += 1;
        let key = match node.task.status {
            TaskStatus::Open => "open",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Deferred => "deferred",
            TaskStatus::Closed => "closed",
            TaskStatus::Canceled => "canceled",
        };
        *summary.get_mut(key).expect("status key exists") += 1;
        for child in &node.children {
            visit(child, summary);
        }
    }

    for node in nodes {
        visit(node, &mut summary);
    }
    summary
}

pub fn print_history(data: &HistoryResult) {
    if data.events.is_empty() {
        println!("{}", style::muted("no events"));
        return;
    }
    for event in &data.events {
        let event_id = event
            .id
            .as_ref()
            .or(event.event_id.as_ref())
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        println!(
            "{} {} {}={} [{}]",
            event.ts,
            style::flow(event_type_to_string(event.event_type)),
            style::key("by"),
            event.actor,
            style::muted(&event_id)
        );
    }
    if data.truncated {
        println!(
            "{}",
            style::muted(&format!(
                "(showing {}, use --limit to see more)",
                data.count
            ))
        );
    }
}

pub fn print_label_list(labels: &[crate::app::service_types::LabelCount]) {
    if labels.is_empty() {
        println!("{}", style::muted("no labels"));
        return;
    }
    for entry in labels {
        println!("{} ({})", style::meta(&entry.label), entry.count);
    }
}

pub fn print_task_note(task_id: &str, note: &TaskNote) {
    println!(
        "{} {}",
        style::task_id(task_id),
        style::success("note added")
    );
    println!(
        "{} {}={} [{}]",
        note.ts,
        style::key("by"),
        note.actor,
        style::muted(&note.event_id)
    );
    println!("{}", note.text);
}

pub fn print_task_notes(task_id: &str, notes: &[TaskNote]) {
    if notes.is_empty() {
        println!("{}: {}", style::task_id(task_id), style::muted("no notes"));
        return;
    }
    println!(
        "{} {}={}",
        style::task_id(task_id),
        style::key("notes"),
        notes.len()
    );
    for note in notes {
        println!(
            "{} {}={} [{}]",
            note.ts,
            style::key("by"),
            note.actor,
            style::muted(&note.event_id)
        );
        println!("{}", note.text);
    }
}

pub fn print_orphans_result(result: &OrphansResult) {
    if result.total == 0 {
        println!("{}", style::success("clean — no orphaned deps or links"));
        return;
    }
    if !result.orphaned_deps.is_empty() {
        println!("orphaned_deps={}", result.orphaned_deps.len());
        for dep in &result.orphaned_deps {
            println!(
                "  {} -> {} ({})",
                dep.child,
                dep.blocker,
                dep_type_to_string(dep.dep_type)
            );
        }
    }
    if !result.orphaned_links.is_empty() {
        println!("orphaned_links={}", result.orphaned_links.len());
        for link in &result.orphaned_links {
            println!("  {} -[{}]-> {}", link.src, link.rel_type, link.dst);
        }
    }
    println!("total={}", result.total);
}

pub fn print_dep_tree_result(root: &DepTreeNode) {
    print_dep_node(root, "", true, true);
}

fn print_dep_node(node: &DepTreeNode, prefix: &str, is_last: bool, is_root: bool) {
    let connector = if is_root {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };
    let dir_tag = if node.direction == crate::domain::dep_tree::DepDirection::Both {
        "".to_string()
    } else {
        format!(" [{}]", dep_direction_to_string(node.direction))
    };
    let type_tag = node
        .dep_type
        .map(|value| format!(" ({})", dep_type_to_string(value)))
        .unwrap_or_default();
    println!(
        "{}{}{} {} {}{}{}",
        style::tree_prefix(prefix),
        style::tree_prefix(connector),
        format_status(node.task.status),
        style::task_id(&node.task.id),
        node.task.title,
        dir_tag,
        type_tag
    );
    let child_prefix = if is_root {
        prefix.to_string()
    } else if is_last {
        format!("{}    ", prefix)
    } else {
        format!("{}│   ", prefix)
    };
    for (index, child) in node.children.iter().enumerate() {
        print_dep_node(
            child,
            &child_prefix,
            index + 1 == node.children.len(),
            false,
        );
    }
}

fn parse_status_label(value: &str) -> Option<TaskStatus> {
    match value {
        "open" => Some(TaskStatus::Open),
        "in_progress" => Some(TaskStatus::InProgress),
        "blocked" => Some(TaskStatus::Blocked),
        "closed" => Some(TaskStatus::Closed),
        "canceled" => Some(TaskStatus::Canceled),
        "deferred" => Some(TaskStatus::Deferred),
        _ => None,
    }
}

fn task_kind_to_string(kind: crate::types::TaskKind) -> &'static str {
    match kind {
        crate::types::TaskKind::Task => "task",
        crate::types::TaskKind::Feature => "feature",
        crate::types::TaskKind::Epic => "epic",
    }
}

pub fn status_to_string(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Open => "open",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Closed => "closed",
        TaskStatus::Canceled => "canceled",
        TaskStatus::Deferred => "deferred",
    }
}

fn planning_state_to_string(state: crate::types::PlanningState) -> &'static str {
    match state {
        crate::types::PlanningState::NeedsPlanning => "needs_planning",
        crate::types::PlanningState::Planned => "planned",
    }
}

fn dep_type_to_string(dep_type: crate::types::DependencyType) -> &'static str {
    match dep_type {
        crate::types::DependencyType::Blocks => "blocks",
        crate::types::DependencyType::StartsAfter => "starts_after",
    }
}

fn relation_type_to_string(rel_type: crate::types::RelationType) -> &'static str {
    match rel_type {
        crate::types::RelationType::RelatesTo => "relates_to",
        crate::types::RelationType::RepliesTo => "replies_to",
        crate::types::RelationType::Duplicates => "duplicates",
        crate::types::RelationType::Supersedes => "supersedes",
    }
}

fn dep_direction_to_string(direction: crate::domain::dep_tree::DepDirection) -> &'static str {
    match direction {
        crate::domain::dep_tree::DepDirection::Up => "up",
        crate::domain::dep_tree::DepDirection::Down => "down",
        crate::domain::dep_tree::DepDirection::Both => "both",
    }
}

fn event_type_to_string(event_type: crate::types::EventType) -> &'static str {
    match event_type {
        crate::types::EventType::TaskCreated => "task.created",
        crate::types::EventType::TaskUpdated => "task.updated",
        crate::types::EventType::TaskStatusSet => "task.status_set",
        crate::types::EventType::TaskClaimed => "task.claimed",
        crate::types::EventType::TaskNoted => "task.noted",
        crate::types::EventType::TaskSpecAttached => "task.spec_attached",
        crate::types::EventType::TaskSuperseded => "task.superseded",
        crate::types::EventType::DepAdded => "dep.added",
        crate::types::EventType::DepRemoved => "dep.removed",
        crate::types::EventType::LinkAdded => "link.added",
        crate::types::EventType::LinkRemoved => "link.removed",
    }
}
