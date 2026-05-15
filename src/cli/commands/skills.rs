use crate::app::service::TasqueService;
use crate::app::service_types::SkillsRefreshInput;
use crate::cli::action::{GlobalOpts, run_action};
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum SkillsCommand {
    /// Refresh managed skill files across all targets
    Refresh,
}

pub fn execute_skills(service: &TasqueService, command: SkillsCommand, opts: GlobalOpts) -> i32 {
    match command {
        SkillsCommand::Refresh => run_action(
            "tsq skills refresh",
            opts,
            || {
                service.skills_refresh(SkillsRefreshInput {
                    source_root_dir: None,
                    home_dir: None,
                    codex_home: None,
                })
            },
            // run_action's JSON mapper returns an owned serializable value; returning
            // `data` by reference does not satisfy the generic lifetime, so clone here.
            |data| data.clone(),
            |data| {
                for result in &data.results {
                    println!(
                        "skill {} {} {}{}",
                        result.target,
                        result.status,
                        result.path,
                        result
                            .message
                            .as_ref()
                            .map(|m| format!(" ({})", m))
                            .unwrap_or_default()
                    );
                }
                Ok(())
            },
        ),
    }
}
