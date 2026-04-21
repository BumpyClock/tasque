use crate::app::service::TasqueService;
use crate::app::service_types::CreateInput;
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::parsers::{
    as_optional_string, parse_kind, parse_planning_state, parse_priority_value,
    validate_explicit_id,
};
use crate::cli::render::print_task;
use crate::errors::TsqError;
use clap::{ArgAction, Args};
use std::collections::HashSet;
use std::fs;

#[derive(Debug, Args)]
pub struct CreateArgs {
    pub title: Option<String>,
    #[arg(long, default_value = "task")]
    pub kind: String,
    #[arg(short = 'p', long = "priority", default_value = "2")]
    pub priority: String,
    #[arg(
        long = "child",
        value_name = "TITLE",
        value_delimiter = ',',
        action = ArgAction::Append
    )]
    pub children: Vec<String>,
    #[arg(long)]
    pub parent: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long = "external-ref")]
    pub external_ref: Option<String>,
    #[arg(long = "discovered-from")]
    pub discovered_from: Option<String>,
    #[arg(long = "planning")]
    pub planning: Option<String>,
    #[arg(long = "needs-planning", default_value_t = false)]
    pub needs_planning: bool,
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
            if args.planning.is_some() && args.needs_planning {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "cannot combine --planning with --needs-planning",
                    1,
                ));
            }
            if !args.children.is_empty() && args.parent.is_none() {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "--child requires --parent",
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
            let mut titles = Vec::new();
            if let Some(title) = args.title.as_ref() {
                titles.push(title.clone());
            }
            for value in &args.children {
                let title = as_optional_string(Some(value)).ok_or_else(|| {
                    TsqError::new("VALIDATION_ERROR", "--child values must not be empty", 1)
                })?;
                titles.push(title);
            }
            if titles.is_empty() {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "title is required unless --child is provided",
                    1,
                ));
            }
            let planning_state = if args.needs_planning {
                Some(crate::types::PlanningState::NeedsPlanning)
            } else {
                args.planning
                    .as_deref()
                    .map(parse_planning_state)
                    .transpose()?
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
            if explicit_id.is_some() && titles.len() > 1 {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "cannot combine --id with multiple task titles",
                    1,
                ));
            }

            let description = as_optional_string(args.description.as_deref());
            let external_ref = as_optional_string(args.external_ref.as_deref());
            let discovered_from = as_optional_string(args.discovered_from.as_deref());

            let mut created = Vec::with_capacity(titles.len());
            for (index, title) in titles.into_iter().enumerate() {
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
