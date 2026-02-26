use crate::app::service::TasqueService;
use crate::cli::action::{GlobalOpts, run_action};
use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum HooksCommand {
    /// Install tsq pre-push hook
    Install(HooksInstallArgs),
    /// Uninstall tsq pre-push hook
    Uninstall,
}

#[derive(Debug, Args)]
pub struct HooksInstallArgs {
    /// Overwrite an existing pre-push hook
    #[arg(long)]
    pub force: bool,
}

pub fn execute_hooks(
    service: &TasqueService,
    command: HooksCommand,
    opts: GlobalOpts,
) -> i32 {
    match command {
        HooksCommand::Install(args) => run_action(
            "tsq hooks install",
            opts,
            || service.hooks_install(args.force),
            |data| data.clone(),
            |data| {
                println!("Installed pre-push hook at {}", data.hook_path);
                Ok(())
            },
        ),
        HooksCommand::Uninstall => run_action(
            "tsq hooks uninstall",
            opts,
            || service.hooks_uninstall(),
            |data| data.clone(),
            |data| {
                if data.removed {
                    println!("Removed pre-push hook at {}", data.hook_path);
                } else {
                    println!("No tsq-managed pre-push hook found");
                }
                Ok(())
            },
        ),
    }
}
