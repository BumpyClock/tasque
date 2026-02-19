use crate::app::service::TasqueService;
use crate::app::service_types::DepTreeInput;
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::parsers::{parse_dep_direction, parse_dependency_type, parse_positive_int};
use crate::cli::render::print_dep_tree_result;
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

fn dep_type_to_string(dep_type: crate::types::DependencyType) -> &'static str {
    match dep_type {
        crate::types::DependencyType::Blocks => "blocks",
        crate::types::DependencyType::StartsAfter => "starts_after",
    }
}
