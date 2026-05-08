use crate::app::service::TasqueService;
use crate::app::service_types::LifecycleStatusInput;
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::render::print_task;
use crate::errors::TsqError;
use clap::Args;

#[derive(Debug, Args)]
#[command(after_help = "Examples:
  tsq defer tsq-abc12345 --note \"waiting on design\"
  tsq open tsq-abc12345")]
pub struct NoteStatusArgs {
    pub id: String,
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Debug, Args)]
#[command(after_help = "Examples:
  tsq done tsq-abc12345 --note \"merged\"
  tsq reopen tsq-abc12345 --note \"regression found\"
  tsq cancel tsq-abc12345 --note \"superseded\"")]
pub struct MultiStatusArgs {
    pub ids: Vec<String>,
    #[arg(long)]
    pub note: Option<String>,
}

pub fn execute_done(service: &TasqueService, args: MultiStatusArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq done",
        opts,
        || {
            validate_multi_status_ids(&args.ids)?;
            service.set_lifecycle_status(LifecycleStatusInput {
                ids: args.ids.clone(),
                status: crate::types::TaskStatus::Closed,
                note: args.note.clone(),
                reason: None,
                exact_id: opts.exact_id,
            })
        },
        |data| serde_json::json!({ "tasks": data.tasks, "notes": data.notes }),
        |data| {
            for task in &data.tasks {
                print_task(task);
            }
            Ok(())
        },
    )
}

pub fn execute_reopen(service: &TasqueService, args: MultiStatusArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq reopen",
        opts,
        || {
            validate_multi_status_ids(&args.ids)?;
            service.set_lifecycle_status(LifecycleStatusInput {
                ids: args.ids.clone(),
                status: crate::types::TaskStatus::Open,
                note: args.note.clone(),
                reason: None,
                exact_id: opts.exact_id,
            })
        },
        |data| serde_json::json!({ "tasks": data.tasks, "notes": data.notes }),
        |data| {
            for task in &data.tasks {
                print_task(task);
            }
            Ok(())
        },
    )
}

pub fn execute_defer(service: &TasqueService, args: NoteStatusArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq defer",
        opts,
        || {
            service.set_lifecycle_status(LifecycleStatusInput {
                ids: vec![args.id.clone()],
                status: crate::types::TaskStatus::Deferred,
                note: args.note.clone(),
                reason: None,
                exact_id: opts.exact_id,
            })
        },
        |data| serde_json::json!({ "tasks": data.tasks, "notes": data.notes }),
        |data| {
            for task in &data.tasks {
                print_task(task);
            }
            Ok(())
        },
    )
}

pub fn execute_cancel(service: &TasqueService, args: MultiStatusArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq cancel",
        opts,
        || {
            validate_multi_status_ids(&args.ids)?;
            service.set_lifecycle_status(LifecycleStatusInput {
                ids: args.ids.clone(),
                status: crate::types::TaskStatus::Canceled,
                note: args.note.clone(),
                reason: None,
                exact_id: opts.exact_id,
            })
        },
        |data| serde_json::json!({ "tasks": data.tasks, "notes": data.notes }),
        |data| {
            for task in &data.tasks {
                print_task(task);
            }
            Ok(())
        },
    )
}

fn validate_multi_status_ids(ids: &[String]) -> Result<(), TsqError> {
    if ids.is_empty() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "at least one task id is required",
            1,
        ));
    }
    Ok(())
}
