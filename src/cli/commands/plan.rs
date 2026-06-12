use crate::app::service::TasqueService;
use crate::app::service_types::{CreateBatchInput, CreateBatchItem};
use crate::app::stdin::read_stdin_content;
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::parsers::{parse_kind, parse_priority_value};
use crate::cli::render::print_task_list_plain;
use crate::errors::TsqError;
use crate::types::{PlanningState, Task};
use clap::Args;
use serde::Serialize;
use std::fs;

#[derive(Debug, Args)]
#[command(after_help = "Examples:
  tsq plan tsq-1 --from plan.md
  tsq plan tsq-1 --from plan.md --apply
  cat plan.md | tsq plan tsq-1 --from - --apply

plan.md format:
  - Build CLI root handling #cli
    - Add strict root tests #test
  ## Show command cleanup
  - Make tsq show <id> show full context")]
pub struct PlanArgs {
    pub parent: String,
    #[arg(long = "from", value_name = "PATH")]
    pub from: String,
    #[arg(long, default_value_t = false)]
    pub apply: bool,
    #[arg(long, default_value = "task")]
    pub kind: String,
    #[arg(short = 'p', long = "priority", default_value = "2")]
    pub priority: String,
    #[arg(long = "planned", default_value_t = false)]
    pub planned: bool,
    #[arg(long = "needs-plan", default_value_t = false)]
    pub needs_plan: bool,
    #[arg(long, default_value_t = true)]
    pub ensure: bool,
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanItemPreview {
    pub title: String,
    pub labels: Vec<String>,
    pub depth: usize,
    pub line: usize,
}

#[derive(Debug, Serialize)]
pub struct PlanResult {
    pub applied: bool,
    pub parent: String,
    pub items: Vec<PlanItemPreview>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<Task>,
}

pub fn execute_plan(service: &TasqueService, args: PlanArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq plan",
        opts,
        || {
            let kind = parse_kind(&args.kind)?;
            let priority = parse_priority_value(&args.priority)?;
            if args.planned && args.needs_plan {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "cannot combine --planned with --needs-plan",
                    1,
                ));
            }
            let content = read_plan_content(&args.from)?;
            let items = parse_plan_items(&content)?;
            let planning_state = if args.needs_plan {
                Some(PlanningState::NeedsPlanning)
            } else if args.planned {
                Some(PlanningState::Planned)
            } else {
                None
            };

            let tasks = if args.apply {
                let batch_items = items
                    .iter()
                    .map(|item| CreateBatchItem {
                        title: item.title.clone(),
                        labels: item.labels.clone(),
                        depth: item.depth,
                        // The marker field preserves source line for diagnostics and traceability.
                        marker: Some(item.line),
                    })
                    .collect();
                service.create_batch(CreateBatchInput {
                    items: batch_items,
                    kind,
                    priority,
                    description: None,
                    external_ref: None,
                    discovered_from: None,
                    parent: Some(args.parent.clone()),
                    exact_id: opts.exact_id,
                    planning_state,
                    body_file: None,
                    ensure: args.ensure && !args.force,
                    force: args.force,
                    from_file: true,
                })?
            } else {
                Vec::new()
            };

            Ok(PlanResult {
                applied: args.apply,
                parent: args.parent.clone(),
                items,
                tasks,
            })
        },
        |result| serde_json::json!({ "plan": result }),
        |result| {
            if opts.plain() {
                if result.applied {
                    print_task_list_plain(&result.tasks);
                } else {
                    print_plan_preview_plain(&result.items);
                }
            } else if result.applied {
                for task in &result.tasks {
                    println!(
                        "{}\t{}\t{}",
                        task.id,
                        task.parent_id.as_deref().unwrap_or("-"),
                        task.title
                    );
                }
            } else {
                println!("preview parent={}", result.parent);
                for item in &result.items {
                    println!(
                        "line={} depth={} title={} labels={}",
                        item.line,
                        item.depth,
                        item.title,
                        item.labels.join(",")
                    );
                }
                println!("run with --apply to create tasks");
            }
            Ok(())
        },
    )
}

fn read_plan_content(path: &str) -> Result<String, TsqError> {
    if path == "-" {
        return read_stdin_content();
    }
    fs::read_to_string(path).map_err(|error| {
        TsqError::new("IO_ERROR", "failed reading plan file", 2)
            .with_details(serde_json::json!({"path": path, "message": error.to_string()}))
    })
}

fn parse_plan_items(content: &str) -> Result<Vec<PlanItemPreview>, TsqError> {
    let mut items = Vec::new();
    let mut previous_depth: Option<usize> = None;
    for (index, line) in content.lines().enumerate() {
        let line_no = index + 1;
        let Some((depth, raw_title)) = parse_plan_line(line, line_no)? else {
            continue;
        };
        if let Some(prev) = previous_depth
            && depth > prev + 1
        {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                format!(
                    "line {} indentation jumps from depth {} to depth {}",
                    line_no, prev, depth
                ),
                1,
            ));
        }
        let (title, labels) = extract_labels(raw_title);
        if title.trim().is_empty() {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                format!("line {} task title must not be empty", line_no),
                1,
            ));
        }
        items.push(PlanItemPreview {
            title: title.trim().to_string(),
            labels,
            depth,
            line: line_no,
        });
        previous_depth = Some(depth);
    }
    if items.is_empty() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "plan file contains no task bullets or headings",
            1,
        ));
    }
    Ok(items)
}

fn parse_plan_line(line: &str, line_no: usize) -> Result<Option<(usize, &str)>, TsqError> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if let Some(rest) = trimmed.strip_prefix('#') {
        let level = trimmed.chars().take_while(|ch| *ch == '#').count();
        let title = rest.trim_start_matches('#').trim_start();
        if title.is_empty() {
            return Ok(None);
        }
        return Ok(Some((level.saturating_sub(1), title)));
    }
    let indent = line.len().saturating_sub(trimmed.len());
    if line[..indent].contains('\t') {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            format!("line {} tab indentation is not supported", line_no),
            1,
        ));
    }
    if indent % 2 != 0 {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            format!(
                "line {} indentation must use multiples of 2 spaces",
                line_no
            ),
            1,
        ));
    }
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            return Ok(Some((indent / 2, strip_checkbox(rest))));
        }
    }
    Ok(None)
}

fn strip_checkbox(value: &str) -> &str {
    let trimmed = value.trim_start();
    for prefix in ["[ ]", "[x]", "[X]"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return rest.trim_start();
        }
    }
    trimmed
}

fn extract_labels(raw: &str) -> (String, Vec<String>) {
    let mut title_parts = Vec::new();
    let mut labels = Vec::new();
    for token in raw.split_whitespace() {
        if let Some(label) = token.strip_prefix('#')
            && !label.is_empty()
            // Labels allow namespaced/hierarchical tokens (`team:ux`, `area/cli`) without key-value semantics.
            && label
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '_' | '/' | '-'))
        {
            labels.push(label.to_ascii_lowercase());
            continue;
        }
        title_parts.push(token);
    }
    labels.sort();
    labels.dedup();
    (title_parts.join(" "), labels)
}

fn print_plan_preview_plain(items: &[PlanItemPreview]) {
    for item in items {
        println!(
            "{}\t{}\t{}\t{}",
            item.line,
            item.depth,
            item.title,
            item.labels.join(",")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_labels, parse_plan_items};

    #[test]
    fn parses_bullets_headings_checkboxes_and_labels() {
        let input = "# Parent #cli\n  - [ ] Child #test\n\nprose\n## Heading";
        let items = parse_plan_items(input).expect("parse");
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].title, "Parent");
        assert_eq!(items[0].depth, 0);
        assert_eq!(items[0].labels, vec!["cli"]);
        assert_eq!(items[1].title, "Child");
        assert_eq!(items[1].depth, 1);
        assert_eq!(items[2].title, "Heading");
        assert_eq!(items[2].depth, 1);
    }

    #[test]
    fn extracts_labels_without_leaving_tag_text() {
        let (title, labels) = extract_labels("Build root #cli #UX");
        assert_eq!(title, "Build root");
        assert_eq!(labels, vec!["cli", "ux"]);
    }
}
