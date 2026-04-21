use crate::app::service::TasqueService;
use crate::app::service_types::{DuplicateInput, MergeInput, SupersedeInput};
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::parsers::parse_positive_int;
use crate::cli::render::{print_merge_result, print_task};
use clap::Args;

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
