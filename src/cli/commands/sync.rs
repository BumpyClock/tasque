use crate::app::service::TasqueService;
use crate::app::sync::DEFAULT_SYNC_BRANCH;
use crate::cli::action::{GlobalOpts, run_action};
use crate::store::merge_driver::merge_events_files;
use clap::Args;
use std::path::Path;

#[derive(Debug, Args)]
pub struct MergeDriverArgs {
    /// Path to ancestor (base) file (%O)
    pub ancestor: String,
    /// Path to ours (current branch) file (%A)
    pub ours: String,
    /// Path to theirs (incoming branch) file (%B)
    pub theirs: String,
}

#[derive(Debug, Args)]
pub struct MigrateArgs {
    /// Name of the sync branch/worktree to migrate events into
    #[arg(long = "sync-branch", alias = "worktree-name")]
    pub sync_branch: Option<String>,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    /// Commit changes but skip pushing to upstream
    #[arg(long = "no-push")]
    pub no_push: bool,
}

/// Execute the merge-driver command.
///
/// This is invoked by git during a merge when the `.gitattributes` file
/// specifies `merge=tasque-events` for `events.jsonl`.
///
/// Does NOT need TasqueService -- operates directly on raw files.
/// Returns 0 on success, 1 on conflict, or `error.exit_code` on merge errors.
pub fn execute_merge_driver(args: MergeDriverArgs) -> i32 {
    let ancestor = Path::new(&args.ancestor);
    let ours = Path::new(&args.ours);
    let theirs = Path::new(&args.theirs);

    match merge_events_files(ancestor, ours, theirs) {
        Ok(outcome) => {
            if outcome.conflict {
                eprintln!(
                    "MERGE_CONFLICT: {} event(s) have divergent payloads:",
                    outcome.conflicting_ids.len()
                );
                for id in &outcome.conflicting_ids {
                    eprintln!("  - {}", id);
                }
                1
            } else {
                eprintln!(
                    "Merged {} events ({} duplicates removed)",
                    outcome.total_events, outcome.duplicates_removed
                );
                0
            }
        }
        Err(error) => {
            eprintln!("{}: {}", error.code, error.message);
            if let Some(details) = &error.details {
                eprintln!("{}", details);
            }
            error.exit_code
        }
    }
}

pub fn execute_migrate(service: &TasqueService, args: MigrateArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq migrate",
        opts,
        || service.migrate(args.sync_branch.as_deref().unwrap_or(DEFAULT_SYNC_BRANCH)),
        |data| data.clone(),
        |data| {
            println!(
                "Migrated {} events to branch '{}' (worktree: {})",
                data.events_migrated, data.branch, data.worktree_path
            );
            Ok(())
        },
    )
}

pub fn execute_sync(service: &TasqueService, args: SyncArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq sync",
        opts,
        || service.sync(!args.no_push),
        |data| data.clone(),
        |data| {
            if data.committed {
                println!(
                    "Committed task updates on '{}' (worktree: {})",
                    data.branch, data.worktree_path
                );
            } else {
                println!("No task updates to commit");
            }

            if data.pushed {
                println!("Pushed '{}' to upstream", data.branch);
            } else if data.has_upstream {
                println!("Skipped push");
            } else {
                println!("No upstream configured; skipped push");
            }
            Ok(())
        },
    )
}
