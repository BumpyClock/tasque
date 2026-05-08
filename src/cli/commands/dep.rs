use crate::app::service::TasqueService;
use crate::app::service_types::{DepInput, DepTreeInput};
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::parsers::{parse_dep_direction, parse_dependency_type, parse_positive_int};
use crate::cli::render::print_dep_tree_result;
use crate::errors::TsqError;
use crate::types::DependencyType;
use clap::{Args, Subcommand};
use serde::Serialize;

#[derive(Debug, Subcommand)]
pub enum DepCommand {
    Add(DepAddArgs),
    Remove(DepRemoveArgs),
    Tree(DepTreeArgs),
}

#[derive(Debug, Args)]
pub struct DepAddArgs {
    pub child: String,
    pub blocker: String,
    #[arg(long = "type", default_value = "blocks")]
    pub dep_type: String,
}

#[derive(Debug, Args)]
pub struct DepRemoveArgs {
    pub child: String,
    pub blocker: String,
    #[arg(long = "type", default_value = "blocks")]
    pub dep_type: String,
}

#[derive(Debug, Args)]
pub struct DepTreeArgs {
    pub id: String,
    #[arg(long, default_value = "both")]
    pub direction: String,
    #[arg(long)]
    pub depth: Option<String>,
}

#[derive(Debug, Args)]
pub struct BlockArgs {
    pub child: String,
    pub by: String,
    pub blocker: String,
}

#[derive(Debug, Args)]
pub struct UnblockArgs {
    pub child: String,
    pub by: String,
    pub blocker: String,
}

#[derive(Debug, Args)]
pub struct OrderArgs {
    pub later: String,
    pub after: String,
    pub earlier: String,
}

#[derive(Debug, Args)]
pub struct UnorderArgs {
    pub later: String,
    pub after: String,
    pub earlier: String,
}

#[derive(Debug, Args)]
pub struct DepsArgs {
    pub id: String,
    #[arg(long, default_value = "both")]
    pub direction: String,
    #[arg(long)]
    pub depth: Option<String>,
}

#[derive(Debug, Serialize)]
struct DepMutationJson {
    child: String,
    blocker: String,
    dep_type: String,
}

pub fn execute_dep(service: &TasqueService, command: DepCommand, opts: GlobalOpts) -> i32 {
    match command {
        DepCommand::Add(args) => run_action(
            "tsq dep add",
            opts,
            || {
                let dep_type = parse_dependency_type(&args.dep_type)?;
                service.dep_add(crate::app::service_types::DepInput {
                    child: args.child.clone(),
                    blocker: args.blocker.clone(),
                    dep_type: Some(dep_type),
                    exact_id: opts.exact_id,
                })
            },
            |(child, blocker, dep_type)| DepMutationJson {
                child: child.clone(),
                blocker: blocker.clone(),
                dep_type: dep_type_to_string(*dep_type).to_string(),
            },
            |(child, blocker, dep_type)| {
                println!(
                    "added dep {} -> {} ({})",
                    child,
                    blocker,
                    dep_type_to_string(*dep_type)
                );
                Ok(())
            },
        ),
        DepCommand::Remove(args) => run_action(
            "tsq dep remove",
            opts,
            || {
                let dep_type = parse_dependency_type(&args.dep_type)?;
                service.dep_remove(crate::app::service_types::DepInput {
                    child: args.child.clone(),
                    blocker: args.blocker.clone(),
                    dep_type: Some(dep_type),
                    exact_id: opts.exact_id,
                })
            },
            |(child, blocker, dep_type)| DepMutationJson {
                child: child.clone(),
                blocker: blocker.clone(),
                dep_type: dep_type_to_string(*dep_type).to_string(),
            },
            |(child, blocker, dep_type)| {
                println!(
                    "removed dep {} -> {} ({})",
                    child,
                    blocker,
                    dep_type_to_string(*dep_type)
                );
                Ok(())
            },
        ),
        DepCommand::Tree(args) => run_action(
            "tsq dep tree",
            opts,
            || {
                let direction = parse_dep_direction(Some(&args.direction))?;
                let depth = args
                    .depth
                    .as_deref()
                    .map(|value| parse_positive_int(value, "depth", 1, 100))
                    .transpose()?
                    .map(|value| value as usize);
                service.dep_tree(DepTreeInput {
                    id: args.id.clone(),
                    direction,
                    depth,
                    exact_id: opts.exact_id,
                })
            },
            |root| serde_json::json!({ "root": root }),
            |root| {
                print_dep_tree_result(root);
                Ok(())
            },
        ),
    }
}

pub fn execute_block(service: &TasqueService, args: BlockArgs, opts: GlobalOpts) -> i32 {
    run_dep_mutation(
        service,
        "tsq block",
        opts,
        || {
            validate_sentence_token(&args.by, "by", "tsq block <task> by <blocker>")?;
            Ok(DepInput {
                child: args.child.clone(),
                blocker: args.blocker.clone(),
                dep_type: Some(DependencyType::Blocks),
                exact_id: opts.exact_id,
            })
        },
        "added",
    )
}

pub fn execute_unblock(service: &TasqueService, args: UnblockArgs, opts: GlobalOpts) -> i32 {
    run_dep_remove(
        service,
        "tsq unblock",
        opts,
        || {
            validate_sentence_token(&args.by, "by", "tsq unblock <task> by <blocker>")?;
            Ok(DepInput {
                child: args.child.clone(),
                blocker: args.blocker.clone(),
                dep_type: Some(DependencyType::Blocks),
                exact_id: opts.exact_id,
            })
        },
        "removed",
    )
}

pub fn execute_order(service: &TasqueService, args: OrderArgs, opts: GlobalOpts) -> i32 {
    run_dep_mutation(
        service,
        "tsq order",
        opts,
        || {
            validate_sentence_token(&args.after, "after", "tsq order <later> after <earlier>")?;
            Ok(DepInput {
                child: args.later.clone(),
                blocker: args.earlier.clone(),
                dep_type: Some(DependencyType::StartsAfter),
                exact_id: opts.exact_id,
            })
        },
        "added",
    )
}

pub fn execute_unorder(service: &TasqueService, args: UnorderArgs, opts: GlobalOpts) -> i32 {
    run_dep_remove(
        service,
        "tsq unorder",
        opts,
        || {
            validate_sentence_token(&args.after, "after", "tsq unorder <later> after <earlier>")?;
            Ok(DepInput {
                child: args.later.clone(),
                blocker: args.earlier.clone(),
                dep_type: Some(DependencyType::StartsAfter),
                exact_id: opts.exact_id,
            })
        },
        "removed",
    )
}

pub fn execute_deps(service: &TasqueService, args: DepsArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq deps",
        opts,
        || {
            let direction = parse_dep_direction(Some(&args.direction))?;
            let depth = args
                .depth
                .as_deref()
                .map(|value| parse_positive_int(value, "depth", 1, 100))
                .transpose()?
                .map(|value| value as usize);
            service.dep_tree(DepTreeInput {
                id: args.id.clone(),
                direction,
                depth,
                exact_id: opts.exact_id,
            })
        },
        |root| serde_json::json!({ "root": root }),
        |root| {
            print_dep_tree_result(root);
            Ok(())
        },
    )
}

pub fn validate_sentence_token(
    actual: &str,
    expected: &str,
    example: &'static str,
) -> Result<(), TsqError> {
    if actual == expected {
        return Ok(());
    }
    Err(TsqError::new(
        "VALIDATION_ERROR",
        format!("expected `{expected}`; example: {example}"),
        1,
    ))
}

fn run_dep_mutation<F>(
    service: &TasqueService,
    command_line: &'static str,
    opts: GlobalOpts,
    input: F,
    human_verb: &'static str,
) -> i32
where
    F: FnOnce() -> Result<DepInput, TsqError>,
{
    run_action(
        command_line,
        opts,
        || service.dep_add(input()?),
        |(child, blocker, dep_type)| DepMutationJson {
            child: child.clone(),
            blocker: blocker.clone(),
            dep_type: dep_type_to_string(*dep_type).to_string(),
        },
        |(child, blocker, dep_type)| {
            println!(
                "{} dep {} -> {} ({})",
                human_verb,
                child,
                blocker,
                dep_type_to_string(*dep_type)
            );
            Ok(())
        },
    )
}

fn run_dep_remove<F>(
    service: &TasqueService,
    command_line: &'static str,
    opts: GlobalOpts,
    input: F,
    human_verb: &'static str,
) -> i32
where
    F: FnOnce() -> Result<DepInput, TsqError>,
{
    run_action(
        command_line,
        opts,
        || service.dep_remove(input()?),
        |(child, blocker, dep_type)| DepMutationJson {
            child: child.clone(),
            blocker: blocker.clone(),
            dep_type: dep_type_to_string(*dep_type).to_string(),
        },
        |(child, blocker, dep_type)| {
            println!(
                "{} dep {} -> {} ({})",
                human_verb,
                child,
                blocker,
                dep_type_to_string(*dep_type)
            );
            Ok(())
        },
    )
}

fn dep_type_to_string(dep_type: DependencyType) -> &'static str {
    match dep_type {
        DependencyType::Blocks => "blocks",
        DependencyType::StartsAfter => "starts_after",
    }
}
