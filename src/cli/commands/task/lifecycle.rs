use crate::app::service::TasqueService;
use crate::app::service_types::{CloseInput, ReopenInput};
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::render::print_task;
use crate::errors::TsqError;
use clap::Args;

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

pub fn execute_close(service: &TasqueService, args: CloseArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq close",
        opts,
        || {
            if args.ids.is_empty() {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "at least one id is required",
                    1,
                ));
            }
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
            if args.ids.is_empty() {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "at least one id is required",
                    1,
                ));
            }
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
