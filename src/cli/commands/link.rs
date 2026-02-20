use crate::app::service::TasqueService;
use crate::app::service_types::LinkInput;
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::parsers::parse_relation_type;
use clap::{Args, Subcommand};
use serde::Serialize;

#[derive(Debug, Subcommand)]
pub enum LinkCommand {
    Add(LinkAddArgs),
    Remove(LinkRemoveArgs),
}

#[derive(Debug, Args)]
pub struct LinkAddArgs {
    pub src: String,
    pub dst: String,
    #[arg(long = "type")]
    pub rel_type: String,
}

#[derive(Debug, Args)]
pub struct LinkRemoveArgs {
    pub src: String,
    pub dst: String,
    #[arg(long = "type")]
    pub rel_type: String,
}

#[derive(Debug, Serialize)]
struct LinkMutationJson {
    src: String,
    dst: String,
    r#type: String,
}

pub fn execute_link(service: &TasqueService, command: LinkCommand, opts: GlobalOpts) -> i32 {
    match command {
        LinkCommand::Add(args) => run_action(
            "tsq link add",
            opts,
            || {
                let rel_type = parse_relation_type(&args.rel_type)?;
                service.link_add(LinkInput {
                    src: args.src.clone(),
                    dst: args.dst.clone(),
                    rel_type,
                    exact_id: opts.exact_id,
                })
            },
            |(src, dst, rel_type)| LinkMutationJson {
                src: src.clone(),
                dst: dst.clone(),
                r#type: relation_type_to_string(*rel_type).to_string(),
            },
            |(src, dst, rel_type)| {
                println!(
                    "added link {}: {} -> {}",
                    relation_type_to_string(*rel_type),
                    src,
                    dst
                );
                Ok(())
            },
        ),
        LinkCommand::Remove(args) => run_action(
            "tsq link remove",
            opts,
            || {
                let rel_type = parse_relation_type(&args.rel_type)?;
                service.link_remove(LinkInput {
                    src: args.src.clone(),
                    dst: args.dst.clone(),
                    rel_type,
                    exact_id: opts.exact_id,
                })
            },
            |(src, dst, rel_type)| LinkMutationJson {
                src: src.clone(),
                dst: dst.clone(),
                r#type: relation_type_to_string(*rel_type).to_string(),
            },
            |(src, dst, rel_type)| {
                println!(
                    "removed link {}: {} -> {}",
                    relation_type_to_string(*rel_type),
                    src,
                    dst
                );
                Ok(())
            },
        ),
    }
}

fn relation_type_to_string(rel_type: crate::types::RelationType) -> &'static str {
    match rel_type {
        crate::types::RelationType::RelatesTo => "relates_to",
        crate::types::RelationType::RepliesTo => "replies_to",
        crate::types::RelationType::Duplicates => "duplicates",
        crate::types::RelationType::Supersedes => "supersedes",
    }
}
