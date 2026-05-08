use crate::app::service::TasqueService;
use crate::app::service_types::CreateInput;
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::parsers::{
    as_optional_string, parse_kind, parse_priority_value, validate_explicit_id,
};
use crate::cli::render::print_task;
use crate::errors::TsqError;
use clap::Args;
use std::collections::HashSet;
use std::fs;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedTaskBullet {
    title: String,
    depth: usize,
    line_no: usize,
}

#[derive(Debug, Args)]
#[command(after_help = "Examples:
  tsq create \"Investigate flaky sync test\"
  tsq create \"Add release checklist\" --kind feature --priority 1 --planned
  tsq create --from-file tasks.md

tasks.md format:
  - Parent task
    - Child task
      - Grandchild task
  - [ ] Checkbox bullet also works")]
pub struct CreateArgs {
    pub titles: Vec<String>,
    #[arg(long, default_value = "task")]
    pub kind: String,
    #[arg(short = 'p', long = "priority", default_value = "2")]
    pub priority: String,
    #[arg(long = "from-file")]
    pub from_file: Option<String>,
    #[arg(long)]
    pub parent: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long = "external-ref")]
    pub external_ref: Option<String>,
    #[arg(long = "discovered-from")]
    pub discovered_from: Option<String>,
    #[arg(long = "planned", default_value_t = false)]
    pub planned: bool,
    #[arg(long = "needs-plan", default_value_t = false)]
    pub needs_plan: bool,
    #[arg(long = "id")]
    pub explicit_id: Option<String>,
    #[arg(long = "body-file")]
    pub body_file: Option<String>,
    #[arg(long, default_value_t = false)]
    pub ensure: bool,
}

pub fn execute_create(service: &TasqueService, args: CreateArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq create",
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
            if args.explicit_id.is_some() && args.parent.is_some() {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "cannot combine --id with --parent",
                    1,
                ));
            }
            if args.ensure && args.explicit_id.is_some() {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "cannot combine --ensure with --id",
                    1,
                ));
            }
            if as_optional_string(args.description.as_deref()).is_some() && args.body_file.is_some()
            {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "cannot combine --description with --body-file",
                    1,
                ));
            }
            if args.from_file.is_some() && !args.titles.is_empty() {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "cannot combine --from-file with positional titles",
                    1,
                ));
            }
            if args.from_file.is_some() && args.explicit_id.is_some() {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "cannot combine --from-file with --id",
                    1,
                ));
            }

            let parsed_file_tasks = if let Some(path) = args.from_file.as_deref() {
                Some(parse_task_bullets(path)?)
            } else {
                None
            };
            let positional_titles = if parsed_file_tasks.is_none() {
                args.titles
                    .iter()
                    .map(|value| {
                        as_optional_string(Some(value)).ok_or_else(|| {
                            TsqError::new("VALIDATION_ERROR", "task title must not be empty", 1)
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                Vec::new()
            };
            let create_count = parsed_file_tasks
                .as_ref()
                .map(|items| items.len())
                .unwrap_or(positional_titles.len());
            if create_count == 0 {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "at least one title is required",
                    1,
                ));
            }
            let single_only = as_optional_string(args.description.as_deref()).is_some()
                || args.body_file.is_some()
                || args.explicit_id.is_some();
            if single_only && create_count > 1 {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "--description, --body-file, and --id require exactly one task title",
                    1,
                ));
            }
            let planning_state = if args.needs_plan {
                Some(crate::types::PlanningState::NeedsPlanning)
            } else if args.planned {
                Some(crate::types::PlanningState::Planned)
            } else {
                None
            };
            let body_file = if let Some(path) = args.body_file.as_deref() {
                let content = if path == "-" {
                    crate::app::stdin::read_stdin_content()?
                } else {
                    fs::read_to_string(path).map_err(|error| {
                        TsqError::new("IO_ERROR", format!("failed reading body file: {}", path), 2)
                            .with_details(serde_json::json!({
                              "kind": format!("{:?}", error.kind()),
                              "message": error.to_string(),
                            }))
                    })?
                };
                if content.trim().is_empty() {
                    return Err(TsqError::new(
                        "VALIDATION_ERROR",
                        "body file content must not be empty",
                        1,
                    ));
                }
                Some(content)
            } else {
                None
            };
            let explicit_id = args
                .explicit_id
                .as_deref()
                .map(validate_explicit_id)
                .transpose()?;
            let description = as_optional_string(args.description.as_deref());
            let external_ref = as_optional_string(args.external_ref.as_deref());
            let discovered_from = as_optional_string(args.discovered_from.as_deref());

            let mut created = Vec::with_capacity(create_count);
            if let Some(file_tasks) = parsed_file_tasks {
                let mut parent_stack: Vec<String> = Vec::new();
                for item in file_tasks {
                    let parent = if item.depth == 0 {
                        args.parent.clone()
                    } else {
                        Some(parent_stack.get(item.depth - 1).cloned().ok_or_else(|| {
                            TsqError::new(
                                "VALIDATION_ERROR",
                                format!(
                                    "line {} has no parent at depth {}",
                                    item.line_no,
                                    item.depth - 1
                                ),
                                1,
                            )
                        })?)
                    };
                    let task = service.create(CreateInput {
                        title: item.title,
                        kind,
                        priority,
                        description: description.clone(),
                        external_ref: external_ref.clone(),
                        discovered_from: discovered_from.clone(),
                        parent,
                        exact_id: opts.exact_id,
                        planning_state,
                        explicit_id: None,
                        body_file: body_file.clone(),
                        ensure: args.ensure,
                    })?;
                    parent_stack.truncate(item.depth);
                    parent_stack.push(task.id.clone());
                    created.push(task);
                }
            } else {
                for (index, title) in positional_titles.into_iter().enumerate() {
                    created.push(service.create(CreateInput {
                        title,
                        kind,
                        priority,
                        description: description.clone(),
                        external_ref: external_ref.clone(),
                        discovered_from: discovered_from.clone(),
                        parent: args.parent.clone(),
                        exact_id: opts.exact_id,
                        planning_state,
                        explicit_id: if index == 0 {
                            explicit_id.clone()
                        } else {
                            None
                        },
                        body_file: body_file.clone(),
                        ensure: args.ensure,
                    })?);
                }
            }
            if args.ensure {
                let mut seen = HashSet::new();
                created.retain(|task| seen.insert(task.id.clone()));
            }

            Ok(created)
        },
        |tasks| {
            if tasks.len() == 1 {
                serde_json::json!({ "task": tasks[0] })
            } else {
                serde_json::json!({ "tasks": tasks })
            }
        },
        |tasks| {
            for task in tasks {
                print_task(task);
            }
            Ok(())
        },
    )
}

fn parse_task_bullets(path: &str) -> Result<Vec<ParsedTaskBullet>, TsqError> {
    let content = fs::read_to_string(path).map_err(|error| {
        TsqError::new(
            "IO_ERROR",
            format!("failed reading tasks file: {}", path),
            2,
        )
        .with_details(serde_json::json!({
            "kind": format!("{:?}", error.kind()),
            "message": error.to_string(),
        }))
    })?;
    let mut tasks = Vec::new();
    let mut previous_depth: Option<usize> = None;
    for (index, line) in content.lines().enumerate() {
        let line_no = index + 1;
        if line.trim().is_empty() {
            continue;
        }
        let indent_prefix: String = line
            .chars()
            .take_while(|value| *value == ' ' || *value == '\t')
            .collect();
        if indent_prefix.contains('\t') {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                format!("line {} tab indentation is not supported", line_no),
                1,
            ));
        }
        let indent = indent_prefix.len();
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
        let depth = indent / 2;
        if tasks.is_empty() && depth != 0 {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                format!("line {} first bullet must not be indented", line_no),
                1,
            ));
        }
        if let Some(prev_depth) = previous_depth
            && depth > prev_depth + 1
        {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                format!(
                    "line {} indentation jumps from depth {} to depth {}",
                    line_no, prev_depth, depth
                ),
                1,
            ));
        }

        let trimmed = &line[indent..];
        let Some(raw_title) = trimmed.strip_prefix("- ") else {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                format!(
                    "line {} must be a markdown bullet starting with '- '",
                    line_no
                ),
                1,
            ));
        };
        let title = raw_title
            .strip_prefix("[ ] ")
            .or_else(|| raw_title.strip_prefix("[x] "))
            .or_else(|| raw_title.strip_prefix("[X] "))
            .unwrap_or(raw_title)
            .trim();
        if title.is_empty() {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                format!("line {} task title must not be empty", line_no),
                1,
            ));
        }
        tasks.push(ParsedTaskBullet {
            title: title.to_string(),
            depth,
            line_no,
        });
        previous_depth = Some(depth);
    }
    if tasks.is_empty() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "tasks file must contain at least one markdown bullet",
            1,
        ));
    }
    Ok(tasks)
}
