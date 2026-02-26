use crate::app::runtime::normalize_status;
use crate::app::service::TasqueService;
use crate::app::service_types::{
    ClaimInput, CloseInput, CreateInput, DuplicateInput, MergeInput, ReopenInput, SearchInput,
    StaleInput, SupersedeInput, UpdateInput,
};
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::parsers::{
    ListParseInput, apply_tree_defaults, as_optional_string, parse_kind, parse_lane,
    parse_list_filter, parse_non_negative_int, parse_planning_state, parse_positive_int,
    parse_priority_value, validate_explicit_id,
};
use crate::cli::render::{
    print_merge_result, print_show_result, print_task, print_task_list, print_task_tree,
};
use crate::errors::TsqError;
use clap::{ArgAction, Args};
use serde::Serialize;
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

#[derive(Debug, Args)]
pub struct ShowArgs {
    pub id: String,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub assignee: Option<String>,
    #[arg(long, default_value_t = false)]
    pub unassigned: bool,
    #[arg(long = "external-ref")]
    pub external_ref: Option<String>,
    #[arg(long = "discovered-from")]
    pub discovered_from: Option<String>,
    #[arg(long)]
    pub kind: Option<String>,
    #[arg(long)]
    pub label: Option<String>,
    #[arg(long = "label-any", value_delimiter = ',', action = clap::ArgAction::Append)]
    pub label_any: Vec<String>,
    #[arg(long = "created-after")]
    pub created_after: Option<String>,
    #[arg(long = "updated-after")]
    pub updated_after: Option<String>,
    #[arg(long = "closed-after")]
    pub closed_after: Option<String>,
    #[arg(long = "id", value_delimiter = ',', action = clap::ArgAction::Append)]
    pub ids: Vec<String>,
    #[arg(long, default_value_t = false)]
    pub tree: bool,
    #[arg(long, default_value_t = false)]
    pub full: bool,
    #[arg(long = "planning")]
    pub planning: Option<String>,
    #[arg(long = "dep-type")]
    pub dep_type: Option<String>,
    #[arg(long = "dep-direction")]
    pub dep_direction: Option<String>,
}

#[derive(Debug, Args)]
pub struct StaleArgs {
    #[arg(long, default_value = "30")]
    pub days: String,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub assignee: Option<String>,
    #[arg(long)]
    pub limit: Option<String>,
}

#[derive(Debug, Args)]
pub struct ReadyArgs {
    #[arg(long)]
    pub lane: Option<String>,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    pub id: String,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long = "clear-description", default_value_t = false)]
    pub clear_description: bool,
    #[arg(long = "external-ref")]
    pub external_ref: Option<String>,
    #[arg(long = "discovered-from")]
    pub discovered_from: Option<String>,
    #[arg(long = "clear-discovered-from", default_value_t = false)]
    pub clear_discovered_from: bool,
    #[arg(long = "clear-external-ref", default_value_t = false)]
    pub clear_external_ref: bool,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub priority: Option<String>,
    #[arg(long, default_value_t = false)]
    pub claim: bool,
    #[arg(long)]
    pub assignee: Option<String>,
    #[arg(long = "require-spec", default_value_t = false)]
    pub require_spec: bool,
    #[arg(long = "planning")]
    pub planning: Option<String>,
}

#[derive(Debug, Args)]
pub struct DuplicateArgs {
    pub id: String,
    #[arg(long = "of")]
    pub canonical: String,
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Debug, Args)]
pub struct DuplicatesArgs {
    #[arg(long, default_value = "20")]
    pub limit: String,
}

#[derive(Debug, Args)]
pub struct SupersedeArgs {
    pub old_id: String,
    #[arg(long = "with")]
    pub new_id: String,
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Debug, Args)]
pub struct MergeArgs {
    pub sources: Vec<String>,
    #[arg(long = "into")]
    pub into: String,
    #[arg(long)]
    pub reason: Option<String>,
    #[arg(long, default_value_t = false)]
    pub force: bool,
    #[arg(long = "dry-run", default_value_t = false)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct CloseArgs {
    pub ids: Vec<String>,
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Debug, Args)]
pub struct ReopenArgs {
    pub ids: Vec<String>,
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    pub query: String,
}

#[derive(Debug, Serialize)]
pub struct TaskJson<T> {
    pub task: T,
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

pub fn execute_show(service: &TasqueService, args: ShowArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq show",
        opts,
        || service.show(&args.id, opts.exact_id),
        |data| data.clone(),
        |data| {
            print_show_result(data);
            Ok(())
        },
    )
}

pub fn execute_list(service: &TasqueService, args: ListArgs, opts: GlobalOpts) -> i32 {
    let filter = match parse_list_filter(ListParseInput {
        status: args.status.clone(),
        assignee: args.assignee.clone(),
        unassigned: args.unassigned,
        has_assignee_flag: args.assignee.is_some(),
        external_ref: args.external_ref.clone(),
        discovered_from: args.discovered_from.clone(),
        kind: args.kind.clone(),
        label: args.label.clone(),
        label_any: args.label_any.clone(),
        created_after: args.created_after.clone(),
        updated_after: args.updated_after.clone(),
        closed_after: args.closed_after.clone(),
        ids: args.ids.clone(),
        planning: args.planning.clone(),
        dep_type: args.dep_type.clone(),
        dep_direction: args.dep_direction.clone(),
    }) {
        Ok(filter) => filter,
        Err(error) => {
            return run_action(
                "tsq list",
                opts,
                || -> Result<(), TsqError> { Err(error) },
                |_: &()| serde_json::json!({}),
                |_: &()| Ok(()),
            );
        }
    };

    if args.tree {
        run_action(
            "tsq list",
            opts,
            || service.list_tree(&apply_tree_defaults(filter.clone(), args.full)),
            |tree| serde_json::json!({ "tree": tree }),
            |tree| {
                print_task_tree(tree);
                Ok(())
            },
        )
    } else {
        run_action(
            "tsq list",
            opts,
            || {
                if args.full {
                    return Err(TsqError::new(
                        "VALIDATION_ERROR",
                        "--full requires --tree",
                        1,
                    ));
                }
                service.list(&filter)
            },
            |tasks| serde_json::json!({ "tasks": tasks }),
            |tasks| {
                print_task_list(tasks);
                Ok(())
            },
        )
    }
}

pub fn execute_stale(service: &TasqueService, args: StaleArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq stale",
        opts,
        || {
            let days = parse_non_negative_int(&args.days, "days")?;
            let status = args.status.as_deref().map(normalize_status).transpose()?;
            let limit = args
                .limit
                .as_deref()
                .map(|value| parse_positive_int(value, "limit", 1, 10000))
                .transpose()?
                .map(|value| value as usize);
            service.stale(&StaleInput {
                days,
                status,
                assignee: as_optional_string(args.assignee.as_deref()),
                limit,
            })
        },
        |data| data.clone(),
        |data| {
            print_task_list(&data.tasks);
            Ok(())
        },
    )
}

pub fn execute_ready(service: &TasqueService, args: ReadyArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq ready",
        opts,
        || {
            let lane = args.lane.as_deref().map(parse_lane).transpose()?;
            service.ready(lane)
        },
        |tasks| serde_json::json!({ "tasks": tasks }),
        |tasks| {
            print_task_list(tasks);
            Ok(())
        },
    )
}

pub fn execute_update(service: &TasqueService, args: UpdateArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq update",
        opts,
        || {
            let claim = args.claim;
            let require_spec = args.require_spec;
            let has_description = as_optional_string(args.description.as_deref()).is_some();
            let clear_description = args.clear_description;
            let has_external_ref = as_optional_string(args.external_ref.as_deref()).is_some();
            let has_discovered_from = as_optional_string(args.discovered_from.as_deref()).is_some();
            let clear_external_ref = args.clear_external_ref;
            let clear_discovered_from = args.clear_discovered_from;

            if has_description && clear_description {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "cannot combine --description with --clear-description",
                    1,
                ));
            }
            if has_external_ref && clear_external_ref {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "cannot combine --external-ref with --clear-external-ref",
                    1,
                ));
            }
            if has_discovered_from && clear_discovered_from {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "cannot combine --discovered-from with --clear-discovered-from",
                    1,
                ));
            }
            if !claim && require_spec {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "--require-spec requires --claim",
                    1,
                ));
            }

            if claim {
                if args.title.is_some()
                    || args.status.is_some()
                    || args.priority.is_some()
                    || has_description
                    || clear_description
                    || has_external_ref
                    || clear_external_ref
                    || has_discovered_from
                    || clear_discovered_from
                {
                    return Err(TsqError::new(
                        "VALIDATION_ERROR",
                        "cannot combine --claim with --title/--description/--clear-description/--external-ref/--clear-external-ref/--discovered-from/--clear-discovered-from/--status/--priority",
                        1,
                    ));
                }
                return service.claim(ClaimInput {
                    id: args.id.clone(),
                    assignee: as_optional_string(args.assignee.as_deref()),
                    require_spec,
                    exact_id: opts.exact_id,
                });
            }

            service.update(UpdateInput {
                id: args.id.clone(),
                title: as_optional_string(args.title.as_deref()),
                description: as_optional_string(args.description.as_deref()),
                clear_description,
                external_ref: as_optional_string(args.external_ref.as_deref()),
                discovered_from: as_optional_string(args.discovered_from.as_deref()),
                clear_discovered_from,
                clear_external_ref,
                status: args.status.as_deref().map(normalize_status).transpose()?,
                priority: args
                    .priority
                    .as_deref()
                    .map(parse_priority_value)
                    .transpose()?,
                exact_id: opts.exact_id,
                planning_state: args
                    .planning
                    .as_deref()
                    .map(parse_planning_state)
                    .transpose()?,
            })
        },
        |task| serde_json::json!({ "task": task }),
        |task| {
            print_task(task);
            Ok(())
        },
    )
}

pub fn execute_duplicate(service: &TasqueService, args: DuplicateArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq duplicate",
        opts,
        || {
            service.duplicate(DuplicateInput {
                source: args.id.clone(),
                canonical: args.canonical.clone(),
                reason: args.reason.clone(),
                exact_id: opts.exact_id,
            })
        },
        |task| serde_json::json!({ "task": task }),
        |task| {
            print_task(task);
            Ok(())
        },
    )
}

pub fn execute_duplicates(service: &TasqueService, args: DuplicatesArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq duplicates",
        opts,
        || {
            let limit = parse_positive_int(&args.limit, "limit", 1, 200)? as usize;
            service.duplicate_candidates(Some(limit))
        },
        |data| data.clone(),
        |data| {
            if data.groups.is_empty() {
                println!("no duplicate candidates");
                return Ok(());
            }
            println!("scanned={} groups={}", data.scanned, data.groups.len());
            for group in &data.groups {
                let ids = group
                    .tasks
                    .iter()
                    .map(|task| task.id.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                println!("{}: {}", group.key, ids);
            }
            Ok(())
        },
    )
}

pub fn execute_supersede(service: &TasqueService, args: SupersedeArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq supersede",
        opts,
        || {
            service.supersede(SupersedeInput {
                source: args.old_id.clone(),
                with_id: args.new_id.clone(),
                reason: args.reason.clone(),
                exact_id: opts.exact_id,
            })
        },
        |task| serde_json::json!({ "task": task }),
        |task| {
            print_task(task);
            Ok(())
        },
    )
}

pub fn execute_merge(service: &TasqueService, args: MergeArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq merge",
        opts,
        || {
            service.merge(MergeInput {
                sources: args.sources.clone(),
                into: args.into.clone(),
                reason: args.reason.clone(),
                force: args.force,
                dry_run: args.dry_run,
                exact_id: opts.exact_id,
            })
        },
        |data| data.clone(),
        |data| {
            print_merge_result(data);
            Ok(())
        },
    )
}

pub fn execute_close(service: &TasqueService, args: CloseArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq close",
        opts,
        || {
            service.close(CloseInput {
                ids: args.ids.clone(),
                reason: args.reason.clone(),
                exact_id: opts.exact_id,
            })
        },
        |tasks| serde_json::json!({ "tasks": tasks }),
        |tasks| {
            for task in tasks {
                print_task(task);
            }
            Ok(())
        },
    )
}

pub fn execute_reopen(service: &TasqueService, args: ReopenArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq reopen",
        opts,
        || {
            service.reopen(ReopenInput {
                ids: args.ids.clone(),
                exact_id: opts.exact_id,
            })
        },
        |tasks| serde_json::json!({ "tasks": tasks }),
        |tasks| {
            for task in tasks {
                print_task(task);
            }
            Ok(())
        },
    )
}

pub fn execute_search(service: &TasqueService, args: SearchArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq search",
        opts,
        || {
            service.search(&SearchInput {
                query: args.query.clone(),
            })
        },
        |tasks| serde_json::json!({ "tasks": tasks }),
        |tasks| {
            print_task_list(tasks);
            Ok(())
        },
    )
}
