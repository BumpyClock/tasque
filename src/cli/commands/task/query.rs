use crate::app::runtime::normalize_status;
use crate::app::service::TasqueService;
use crate::app::service_types::{SearchInput, StaleInput};
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::parsers::{
    ListParseInput, apply_tree_defaults, as_optional_string, parse_lane, parse_list_filter,
    parse_non_negative_int, parse_positive_int,
};
use crate::cli::render::{print_show_result, print_task_list, print_task_tree};
use crate::errors::TsqError;
use clap::Args;

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
pub struct SearchArgs {
    pub query: String,
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
