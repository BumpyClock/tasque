use crate::app::service::TasqueService;
use crate::app::service_types::{ListFilter, SearchInput, SimilarInput};
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::parsers::{ListParseInput, apply_tree_defaults, parse_lane, parse_list_filter};
use crate::cli::render::{print_task, print_task_list, print_task_tree};
use crate::errors::TsqError;
use clap::{Args, Subcommand};
use std::collections::HashSet;

#[derive(Debug, Args)]
#[command(after_help = "Examples:
  tsq find ready --lane planning
  tsq find ready --lane coding --label cli
  tsq find open --planning needs_planning --tree
  tsq find search \"sync branch\" --full")]
pub struct FindArgs {
    #[command(subcommand)]
    pub command: FindCommand,
}

#[derive(Debug, Subcommand)]
pub enum FindCommand {
    Ready(FindReadyArgs),
    Open(FindListArgs),
    InProgress(FindListArgs),
    Blocked(FindListArgs),
    Deferred(FindListArgs),
    Done(FindListArgs),
    Canceled(FindListArgs),
    Search(FindSearchArgs),
    Similar(FindSimilarArgs),
}

#[derive(Debug, Args)]
pub struct FindReadyArgs {
    #[arg(long)]
    pub lane: Option<String>,
    #[command(flatten)]
    pub filter: FindListArgs,
}

#[derive(Debug, Args, Default, Clone, PartialEq, Eq)]
pub struct FindListArgs {
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
pub struct FindSearchArgs {
    pub query: String,
    #[arg(long, default_value_t = false)]
    pub full: bool,
}

#[derive(Debug, Args)]
pub struct FindSimilarArgs {
    pub query: String,
}

pub fn execute_find(service: &TasqueService, args: FindArgs, opts: GlobalOpts) -> i32 {
    match args.command {
        FindCommand::Ready(args) => execute_find_ready(service, args, opts),
        FindCommand::Open(args) => {
            execute_find_list(service, args, Some("open"), "tsq find open", opts)
        }
        FindCommand::InProgress(args) => execute_find_list(
            service,
            args,
            Some("in_progress"),
            "tsq find in-progress",
            opts,
        ),
        FindCommand::Blocked(args) => {
            execute_find_list(service, args, Some("blocked"), "tsq find blocked", opts)
        }
        FindCommand::Deferred(args) => {
            execute_find_list(service, args, Some("deferred"), "tsq find deferred", opts)
        }
        FindCommand::Done(args) => {
            execute_find_list(service, args, Some("closed"), "tsq find done", opts)
        }
        FindCommand::Canceled(args) => {
            execute_find_list(service, args, Some("canceled"), "tsq find canceled", opts)
        }
        FindCommand::Search(args) => execute_find_search(service, args, opts),
        FindCommand::Similar(args) => execute_find_similar(service, args, opts),
    }
}

fn execute_find_ready(service: &TasqueService, args: FindReadyArgs, opts: GlobalOpts) -> i32 {
    if args.filter.tree {
        return run_action(
            "tsq find ready",
            opts,
            || {
                let lane = args.lane.as_deref().map(parse_lane).transpose()?;
                let ready = service.ready(lane)?;
                let ready_ids = ready.into_iter().map(|task| task.id).collect::<Vec<_>>();
                let filter = parse_find_list_filter(&args.filter, None)?;
                let filter = filter_to_ready_ids(filter, ready_ids);
                service.list_tree(&apply_tree_defaults(filter, args.filter.full))
            },
            |tree| serde_json::json!({ "tree": tree }),
            |tree| {
                print_task_tree(tree);
                Ok(())
            },
        );
    }

    run_action(
        "tsq find ready",
        opts,
        || {
            let lane = args.lane.as_deref().map(parse_lane).transpose()?;
            let ready = service.ready(lane)?;
            let ready_ids = ready.into_iter().map(|task| task.id).collect::<Vec<_>>();
            let filter = parse_find_list_filter(&args.filter, None)?;
            let filter = filter_to_ready_ids(filter, ready_ids);
            if args.filter.full {
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

fn execute_find_list(
    service: &TasqueService,
    args: FindListArgs,
    status: Option<&str>,
    command_line: &'static str,
    opts: GlobalOpts,
) -> i32 {
    let filter = match parse_find_list_filter(&args, status) {
        Ok(filter) => filter,
        Err(error) => {
            return run_action(
                command_line,
                opts,
                || -> Result<(), TsqError> { Err(error) },
                |_: &()| serde_json::json!({}),
                |_: &()| Ok(()),
            );
        }
    };

    if args.tree {
        run_action(
            command_line,
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
            command_line,
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

pub fn execute_find_search(service: &TasqueService, args: FindSearchArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq find search",
        opts,
        || {
            service.search(&SearchInput {
                query: args.query.clone(),
            })
        },
        |tasks| serde_json::json!({ "tasks": tasks }),
        |tasks| {
            if args.full {
                for task in tasks {
                    print_task(task);
                }
            } else {
                print_task_list(tasks);
            }
            Ok(())
        },
    )
}

pub fn execute_find_similar(
    service: &TasqueService,
    args: FindSimilarArgs,
    opts: GlobalOpts,
) -> i32 {
    run_action(
        "tsq find similar",
        opts,
        || {
            service.similar(&SimilarInput {
                query: args.query.clone(),
            })
        },
        |candidates| serde_json::json!({ "candidates": candidates }),
        |candidates| {
            if !candidates.is_empty() {
                println!("{:>5} {:25} {:12} TITLE", "SCORE", "REASON", "ID");
            }
            for candidate in candidates {
                println!(
                    "{:>5.2} {:25} {:12} {}",
                    candidate.score, candidate.reason, candidate.task.id, candidate.task.title
                );
            }
            Ok(())
        },
    )
}

fn parse_find_list_filter(
    args: &FindListArgs,
    status: Option<&str>,
) -> Result<ListFilter, TsqError> {
    parse_list_filter(ListParseInput {
        status: status.map(ToString::to_string),
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
    })
}

fn filter_to_ready_ids(mut filter: ListFilter, ready_ids: Vec<String>) -> ListFilter {
    let ready_set: HashSet<String> = ready_ids.into_iter().collect();
    let ids = match filter.ids.take() {
        Some(ids) => ids
            .into_iter()
            .filter(|id| ready_set.contains(id))
            .collect::<Vec<_>>(),
        None => {
            let mut ids = ready_set.into_iter().collect::<Vec<_>>();
            ids.sort();
            ids
        }
    };
    filter.ids = Some(ids);
    filter
}
