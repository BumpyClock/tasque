use crate::app::service::TasqueService;
use crate::app::service_types::LabelInput;
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::render::{print_label_list, print_task};
use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum LabelCommand {
    Add(LabelAddArgs),
    Remove(LabelRemoveArgs),
    List,
}

#[derive(Debug, Args)]
pub struct LabelAddArgs {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Args)]
pub struct LabelRemoveArgs {
    pub id: String,
    pub label: String,
}

pub fn execute_label(service: &TasqueService, command: LabelCommand, opts: GlobalOpts) -> i32 {
    match command {
        LabelCommand::Add(args) => run_action(
            "tsq label add",
            opts,
            || {
                service.label_add(LabelInput {
                    id: args.id.clone(),
                    label: args.label.clone(),
                    exact_id: opts.exact_id,
                })
            },
            |task| serde_json::json!({ "task": task }),
            |task| {
                print_task(task);
                Ok(())
            },
        ),
        LabelCommand::Remove(args) => run_action(
            "tsq label remove",
            opts,
            || {
                service.label_remove(LabelInput {
                    id: args.id.clone(),
                    label: args.label.clone(),
                    exact_id: opts.exact_id,
                })
            },
            |task| serde_json::json!({ "task": task }),
            |task| {
                print_task(task);
                Ok(())
            },
        ),
        LabelCommand::List => run_action(
            "tsq label list",
            opts,
            || service.label_list(),
            |labels| serde_json::json!({ "labels": labels }),
            |labels| {
                print_label_list(labels);
                Ok(())
            },
        ),
    }
}
