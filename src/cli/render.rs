use crate::app::service_query::ShowResult;
use crate::app::service_types::{HistoryResult, MergeResult, OrphansResult, SpecContentResult};
use crate::cli::style;
use crate::cli::terminal::{Density, resolve_density, resolve_width};
use crate::domain::dep_tree::DepTreeNode;
use crate::types::{RepairResult, Task, TaskNote, TaskStatus, TaskTreeNode};
use std::collections::HashMap;

pub struct TreeRenderOptions {
    pub width: Option<usize>,
}

const MAX_NARROW_TREE_PREFIX_WIDTH: usize = 24;

/// Escape C0/C1 control characters, DEL, and bidi controls for safe terminal display.
/// `\\`, `\t`, `\r`, `\n` keep their existing readable escapes; ESC and every
/// other byte-sized control character are rendered as `\x{:02x}`. Bidi controls
/// render as `\u{...}` so they cannot reorder nearby terminal text. All other
/// characters (including newlines' surrounding text, Unicode, emoji) pass
/// through unchanged.
pub(crate) fn sanitize_inline(value: &str) -> String {
    sanitize(value, false)
}

/// Same escaping as `sanitize_inline`, but preserves real newlines for
/// multi-line human-output contexts (spec content, note bodies).
pub(crate) fn sanitize_block(value: &str) -> String {
    sanitize(value, true)
}

fn sanitize(value: &str, keep_newlines: bool) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\n' if keep_newlines => out.push('\n'),
            '\n' => out.push_str("\\n"),
            c if is_bidi_control(c) => {
                out.push_str(&format!("\\u{{{:x}}}", c as u32));
            }
            c if is_unsafe_control(c) => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

fn is_unsafe_control(ch: char) -> bool {
    let code = ch as u32;
    code < 0x20 || code == 0x7f || (0x80..=0x9f).contains(&code) || is_bidi_control(ch)
}

fn is_bidi_control(ch: char) -> bool {
    matches!(ch as u32, 0x202a..=0x202e | 0x2066..=0x2069)
}

fn plain_cell(value: &str) -> String {
    sanitize_inline(value)
}

pub fn print_task_list(tasks: &[Task]) {
    if tasks.is_empty() {
        println!("{}", style::muted("no tasks"));
        return;
    }

    let header = ["ID", "ALIAS", "P", "KIND", "STATUS", "ASSIGNEE", "TITLE"];
    let rows: Vec<Vec<String>> = tasks
        .iter()
        .map(|task| {
            vec![
                task.id.clone(),
                sanitize_inline(&task.alias),
                task.priority.to_string(),
                task_kind_to_string(task.kind).to_string(),
                status_to_string(task.status).to_string(),
                task.assignee
                    .as_deref()
                    .map(sanitize_inline)
                    .unwrap_or_else(|| "-".to_string()),
                sanitize_inline(&task.title),
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
                } else if index == 4 {
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

pub fn print_task_list_plain(tasks: &[Task]) {
    for task in tasks {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            plain_cell(&task.id),
            plain_cell(&task.alias),
            task.priority,
            task_kind_to_string(task.kind),
            status_to_string(task.status),
            plain_cell(task.assignee.as_deref().unwrap_or("-")),
            plain_cell(&task.title)
        );
    }
}

pub fn print_task(task: &Task) {
    println!(
        "{} {} {}",
        style::task_id(&task.id),
        sanitize_inline(&task.alias),
        sanitize_inline(&task.title)
    );
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
        println!("{}={}", style::key("assignee"), sanitize_inline(assignee));
    }
    if let Some(external_ref) = &task.external_ref {
        println!(
            "{}={}",
            style::key("external_ref"),
            sanitize_inline(external_ref)
        );
    }
    if let Some(discovered_from) = &task.discovered_from {
        println!(
            "{}={}",
            style::key("discovered_from"),
            sanitize_inline(discovered_from)
        );
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
        println!(
            "{}={}",
            style::key("description"),
            sanitize_inline(description)
        );
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

pub fn print_task_plain(task: &Task) {
    println!("id\t{}", plain_cell(&task.id));
    println!("alias\t{}", plain_cell(&task.alias));
    println!("title\t{}", plain_cell(&task.title));
    println!("kind\t{}", task_kind_to_string(task.kind));
    println!("status\t{}", status_to_string(task.status));
    println!("priority\t{}", task.priority);
    if let Some(planning_state) = task.planning_state {
        println!("planning\t{}", planning_state_to_string(planning_state));
    }
    if let Some(assignee) = &task.assignee {
        println!("assignee\t{}", plain_cell(assignee));
    }
    if let Some(external_ref) = &task.external_ref {
        println!("external_ref\t{}", plain_cell(external_ref));
    }
    if let Some(discovered_from) = &task.discovered_from {
        println!("discovered_from\t{}", plain_cell(discovered_from));
    }
    if let Some(superseded_by) = &task.superseded_by {
        println!("superseded_by\t{}", plain_cell(superseded_by));
    }
    if let Some(duplicate_of) = &task.duplicate_of {
        println!("duplicate_of\t{}", plain_cell(duplicate_of));
    }
    if let Some(parent) = &task.parent_id {
        println!("parent\t{}", plain_cell(parent));
    }
    if let Some(description) = &task.description {
        println!("description\t{}", plain_cell(description));
    }
    println!("notes\t{}", task.notes.len());
    if let (Some(spec_path), Some(spec_fingerprint)) = (&task.spec_path, &task.spec_fingerprint) {
        println!(
            "spec\t{}\t{}",
            plain_cell(spec_path),
            plain_cell(spec_fingerprint)
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

pub fn print_show_result_plain(data: &ShowResult) {
    print_task_plain(&data.task);
    for edge in &data.blocker_edges {
        println!(
            "blocker\t{}\t{}",
            plain_cell(&edge.id),
            dep_type_to_string(edge.dep_type)
        );
    }
    for edge in &data.dependent_edges {
        println!(
            "dependent\t{}\t{}",
            plain_cell(&edge.id),
            dep_type_to_string(edge.dep_type)
        );
    }
    println!("ready\t{}", data.ready);
    if !data.history.is_empty() {
        println!("history_events\t{}", data.history.len());
    }
    for (kind, values) in &data.links {
        for value in values {
            println!("link\t{}\t{}", plain_cell(kind), plain_cell(value));
        }
    }
}

pub fn print_spec_content(data: &SpecContentResult) {
    println!("--- spec: {} ---", data.spec_path);
    let sanitized = sanitize_block(&data.content);
    print!("{}", sanitized);
    if !sanitized.ends_with('\n') {
        println!();
    }
    println!("--- end spec ---");
}

pub fn print_spec_content_plain(data: &SpecContentResult) {
    println!("spec_path\t{}", plain_cell(&data.spec_path));
    println!("spec_content\t{}", plain_cell(&data.content));
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
    let display_line_prefix = compact_tree_prefix(&line_prefix, density, width);
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
    let meta_text = format_meta_badge_text(&node.task);
    let meta = style::meta(&meta_text);
    let prefix_width = display_line_prefix.chars().count();
    let meta_width = meta_text.chars().count();
    let task_id_text = if density == Density::Narrow {
        compact_task_id(
            &node.task.id,
            width,
            prefix_width,
            status_text.len(),
            meta_width,
        )
    } else {
        node.task.id.clone()
    };
    let task_id = style::task_id(&task_id_text);
    let mut primary_parts = vec![status, task_id];
    match density {
        Density::Narrow => {
            primary_parts.push(meta.clone());
            let max_title_width = compute_title_width(
                width,
                prefix_width,
                status_text.len(),
                task_id_text.chars().count(),
                meta_width,
            );
            if max_title_width > 0 {
                primary_parts.push(truncate_with_ellipsis(
                    &sanitize_inline(&node.task.title),
                    max_title_width,
                ));
            }
        }
        Density::Medium | Density::Wide => {
            primary_parts.push(sanitize_inline(&node.task.title));
            primary_parts.push(meta);
            if density == Density::Wide
                && let Some(flow) = &flow
            {
                primary_parts.push(flow.clone());
            }
        }
    }

    lines.push(format!(
        "{}{}",
        style::tree_prefix(&display_line_prefix),
        primary_parts.join(" ")
    ));
    if density == Density::Medium
        && let Some(flow) = &flow
    {
        lines.push(format!("{}{}", style::tree_prefix(&meta_prefix), flow));
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
        sanitize_inline(&result.target.title),
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
                .unwrap_or_else(|| "needs_planning".to_string())
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
    style::meta(&format_meta_badge_text(task))
}

fn format_meta_badge_text(task: &Task) -> String {
    let assignee = task
        .assignee
        .as_ref()
        .map(|value| format!(" @{}", sanitize_inline(value)))
        .unwrap_or_default();
    format!("[p{}{}]", task.priority, assignee)
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
    meta_length: usize,
) -> usize {
    let fixed_length = prefix_length + status_length + 1 + task_id_length + 1 + meta_length + 1;
    width.saturating_sub(fixed_length)
}

fn compact_tree_prefix(prefix: &str, density: Density, width: usize) -> String {
    let limit = narrow_tree_prefix_limit(width);
    if density != Density::Narrow || prefix.chars().count() <= limit {
        return prefix.to_string();
    }

    let keep = limit.saturating_sub(2);
    let start = prefix.chars().count().saturating_sub(keep);
    let suffix: String = prefix.chars().skip(start).collect();
    format!("… {}", suffix)
}

fn narrow_tree_prefix_limit(width: usize) -> usize {
    (width / 3).clamp(8, MAX_NARROW_TREE_PREFIX_WIDTH)
}

fn compact_task_id(
    id: &str,
    width: usize,
    prefix_length: usize,
    status_length: usize,
    meta_length: usize,
) -> String {
    let title_floor = if width >= 50 { 12 } else { 6 };
    let fixed_without_id = prefix_length + status_length + 1 + 1 + meta_length + 1 + title_floor;
    let budget = width.saturating_sub(fixed_without_id);
    let min_budget = id.chars().count().min(6);
    let budget = budget.max(min_budget);
    truncate_middle(id, budget)
}

fn truncate_middle(value: &str, max_length: usize) -> String {
    let len = value.chars().count();
    if len <= max_length {
        return value.to_string();
    }
    if max_length <= 3 {
        return value.chars().take(max_length).collect();
    }
    let keep = max_length - 3;
    let head = keep.div_ceil(2);
    let tail = keep / 2;
    let prefix: String = value.chars().take(head).collect();
    let suffix: String = value
        .chars()
        .rev()
        .take(tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}...{}", prefix, suffix)
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
            style::flow(&event_type_to_string(event.event_type)),
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

pub fn print_history_plain(data: &HistoryResult) {
    for event in &data.events {
        let event_id = event
            .id
            .as_ref()
            .or(event.event_id.as_ref())
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        println!(
            "{}\t{}\t{}\t{}",
            plain_cell(&event.ts),
            event_type_to_string(event.event_type),
            plain_cell(&event.actor),
            plain_cell(&event_id)
        );
    }
    if data.truncated {
        println!(
            "# truncated\tshowing_{}\tuse_--limit_to_see_more",
            data.count
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
    println!("{}", sanitize_block(&note.text));
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
        println!("{}", sanitize_block(&note.text));
    }
}

pub fn print_task_notes_plain(task_id: &str, notes: &[TaskNote]) {
    for note in notes {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            plain_cell(task_id),
            plain_cell(&note.ts),
            plain_cell(&note.actor),
            plain_cell(&note.event_id),
            plain_cell(&note.text)
        );
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
        sanitize_inline(&node.task.title),
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

fn task_kind_to_string(kind: crate::types::TaskKind) -> String {
    crate::domain::event_payload_codecs::task_kind_as_str(kind)
}

pub fn status_to_string(status: TaskStatus) -> String {
    crate::domain::event_payload_codecs::task_status_as_str(status)
}

fn planning_state_to_string(state: crate::types::PlanningState) -> String {
    crate::domain::event_payload_codecs::planning_state_as_str(state)
}

fn dep_type_to_string(dep_type: crate::types::DependencyType) -> String {
    crate::domain::event_payload_codecs::dependency_type_as_str(dep_type)
}

fn relation_type_to_string(rel_type: crate::types::RelationType) -> String {
    crate::domain::event_payload_codecs::relation_type_as_str(rel_type)
}

fn dep_direction_to_string(direction: crate::domain::dep_tree::DepDirection) -> &'static str {
    match direction {
        crate::domain::dep_tree::DepDirection::Up => "up",
        crate::domain::dep_tree::DepDirection::Down => "down",
        crate::domain::dep_tree::DepDirection::Both => "both",
    }
}

fn event_type_to_string(event_type: crate::types::EventType) -> String {
    crate::domain::event_payload_codecs::event_type_as_str(event_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PlanningState, TaskKind};

    #[test]
    fn sanitize_inline_escapes_esc() {
        assert_eq!(sanitize_inline("a\u{1b}b"), "a\\x1bb");
    }

    #[test]
    fn sanitize_inline_escapes_osc_sequence_including_bel() {
        // ESC ] 0 ; x BEL — an OSC window-title sequence.
        let input = "evil\u{1b}]0;pwned\u{7}title";
        let expected = "evil\\x1b]0;pwned\\x07title";
        assert_eq!(sanitize_inline(input), expected);
    }

    #[test]
    fn sanitize_inline_escapes_c1_csi() {
        // U+009B is the single-byte CSI introducer in the C1 control range.
        assert_eq!(sanitize_inline("a\u{9b}b"), "a\\x9bb");
    }

    #[test]
    fn sanitize_inline_escapes_bidi_controls() {
        assert_eq!(sanitize_inline("safe\u{202e}txt"), "safe\\u{202e}txt");
        assert_eq!(sanitize_inline("safe\u{2066}txt"), "safe\\u{2066}txt");
    }

    #[test]
    fn sanitize_inline_escapes_del() {
        assert_eq!(sanitize_inline("a\u{7f}b"), "a\\x7fb");
    }

    #[test]
    fn sanitize_inline_escapes_backslash_tab_cr_lf() {
        assert_eq!(sanitize_inline("a\\b\tc\rd\ne"), "a\\\\b\\tc\\rd\\ne");
    }

    #[test]
    fn sanitize_inline_passes_through_plain_ascii_and_emoji() {
        let input = "Fix auth redirect 🎉 — done!";
        assert_eq!(sanitize_inline(input), input);
    }

    #[test]
    fn sanitize_block_preserves_real_newlines_but_escapes_control_chars() {
        let input = "line one\nline\u{1b}two\nline three";
        let expected = "line one\nline\\x1btwo\nline three";
        assert_eq!(sanitize_block(input), expected);
    }

    #[test]
    fn sanitize_block_still_escapes_backslash_tab_cr() {
        assert_eq!(sanitize_block("a\\b\tc\rd"), "a\\\\b\\tc\\rd");
    }

    #[test]
    fn narrow_tree_lines_fit_terminal_width_for_deep_hierarchies() {
        let width = 80;
        let tree = vec![nested_node(12)];

        let lines = render_task_tree(&tree, TreeRenderOptions { width: Some(width) });

        for line in lines.iter().filter(|line| !line.starts_with("total=")) {
            let visible_width = visible_char_count(line);
            assert!(
                visible_width <= width,
                "line exceeded width {width}: {line:?} ({})",
                visible_width
            );
            assert!(
                !line.trim_start().starts_with("[p"),
                "priority metadata should not render as detached wrapped line: {line:?}"
            );
        }
    }

    #[test]
    fn narrow_tree_compacts_nested_child_ids_when_fixed_parts_are_wide() {
        let width = 40;
        let tree = vec![nested_child_id_node(8, "tsq-root".to_string())];

        let lines = render_task_tree(&tree, TreeRenderOptions { width: Some(width) });

        assert!(
            lines.iter().any(|line| line.contains("...")),
            "expected long nested ids or titles to be compacted\n{}",
            lines.join("\n")
        );
        for line in lines.iter().filter(|line| !line.starts_with("total=")) {
            let visible_width = visible_char_count(line);
            assert!(
                visible_width <= width,
                "line exceeded width {width}: {line:?} ({})",
                visible_width
            );
        }
    }

    fn visible_char_count(value: &str) -> usize {
        let mut count = 0;
        let mut chars = value.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\u{1b}' && chars.peek() == Some(&'[') {
                chars.next();
                for ansi_ch in chars.by_ref() {
                    if ansi_ch.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
            count += 1;
        }
        count
    }

    fn nested_node(depth: usize) -> TaskTreeNode {
        let id = format!("tsq-deepnode{depth}");
        let child = if depth == 0 {
            Vec::new()
        } else {
            vec![nested_node(depth - 1)]
        };
        TaskTreeNode {
            task: task(
                &id,
                "Long task title that used to wrap inside narrow tmux panes",
            ),
            blockers: Vec::new(),
            dependents: Vec::new(),
            blocker_edges: None,
            dependent_edges: None,
            children: child,
        }
    }

    fn nested_child_id_node(depth: usize, id: String) -> TaskTreeNode {
        let child = if depth == 0 {
            Vec::new()
        } else {
            vec![nested_child_id_node(depth - 1, format!("{}.1", id))]
        };
        TaskTreeNode {
            task: task(
                &id,
                "Long task title that used to wrap inside narrow tmux panes",
            ),
            blockers: Vec::new(),
            dependents: Vec::new(),
            blocker_edges: None,
            dependent_edges: None,
            children: child,
        }
    }

    fn task(id: &str, title: &str) -> Task {
        Task {
            id: id.to_string(),
            alias: crate::domain::alias::base_alias(title),
            kind: TaskKind::Task,
            title: title.to_string(),
            description: None,
            notes: Vec::new(),
            spec_path: None,
            spec_fingerprint: None,
            spec_attached_at: None,
            spec_attached_by: None,
            status: TaskStatus::Open,
            priority: 1,
            assignee: None,
            external_ref: None,
            discovered_from: None,
            parent_id: None,
            superseded_by: None,
            duplicate_of: None,
            planning_state: Some(PlanningState::NeedsPlanning),
            replies_to: None,
            labels: Vec::new(),
            created_at: "2026-05-11T00:00:00Z".to_string(),
            updated_at: "2026-05-11T00:00:00Z".to_string(),
            closed_at: None,
        }
    }
}
