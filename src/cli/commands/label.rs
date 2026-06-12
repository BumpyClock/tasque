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

#[derive(Debug, Args)]
pub struct LabelArgs {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Args)]
pub struct UnlabelArgs {
    pub id: String,
    pub label: String,
}

pub fn execute_label(service: &TasqueService, command: LabelCommand, opts: GlobalOpts) -> i32 {
    match command {
        LabelCommand::Add(args) => run_label_mutation(
            service,
            "tsq label add",
            opts,
            args.id,
            args.label,
            TasqueService::label_add,
        ),
        LabelCommand::Remove(args) => run_label_mutation(
            service,
            "tsq label remove",
            opts,
            args.id,
            args.label,
            TasqueService::label_remove,
        ),
        LabelCommand::List => run_label_list(service, "tsq label list", opts),
    }
}

pub fn execute_label_add(service: &TasqueService, args: LabelArgs, opts: GlobalOpts) -> i32 {
    run_label_mutation(
        service,
        "tsq label",
        opts,
        args.id,
        args.label,
        TasqueService::label_add,
    )
}

pub fn execute_unlabel(service: &TasqueService, args: UnlabelArgs, opts: GlobalOpts) -> i32 {
    run_label_mutation(
        service,
        "tsq unlabel",
        opts,
        args.id,
        args.label,
        TasqueService::label_remove,
    )
}

pub fn execute_labels(service: &TasqueService, opts: GlobalOpts) -> i32 {
    run_label_list(service, "tsq labels", opts)
}

fn run_label_mutation(
    service: &TasqueService,
    command_line: &'static str,
    opts: GlobalOpts,
    id: String,
    label: String,
    action: fn(&TasqueService, LabelInput) -> Result<crate::types::Task, crate::errors::TsqError>,
) -> i32 {
    run_action(
        command_line,
        opts,
        || {
            action(
                service,
                LabelInput {
                    id,
                    label,
                    exact_id: opts.exact_id,
                },
            )
        },
        |task| serde_json::json!({ "task": task }),
        |task| {
            print_task(task);
            Ok(())
        },
    )
}

fn run_label_list(service: &TasqueService, command_line: &'static str, opts: GlobalOpts) -> i32 {
    run_action(
        command_line,
        opts,
        || service.label_list(),
        |labels| serde_json::json!({ "labels": labels }),
        |labels| {
            print_label_list(labels);
            Ok(())
        },
    )
}
