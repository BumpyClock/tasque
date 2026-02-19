use crate::app::service::TasqueService;
use crate::app::service_types::HistoryInput;
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::init_flow::{
    InitCommandOptions, InitPlan, InitResolutionContext, resolve_init_plan, run_init_wizard,
};
use crate::cli::parsers::{as_optional_string, parse_positive_int, parse_status_csv};
use crate::cli::render::{print_history, print_orphans_result, print_repair_result};
use crate::cli::watch::{WatchOptions, start_watch};
use crate::errors::TsqError;
use crate::output::err_envelope;
use clap::Args;
use std::io::IsTerminal;

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(long, default_value_t = false)]
    pub wizard: bool,
    #[arg(long = "no-wizard", default_value_t = false)]
    pub no_wizard: bool,
    #[arg(long, default_value_t = false)]
    pub yes: bool,
    #[arg(long)]
    pub preset: Option<String>,
    #[arg(long = "install-skill", default_value_t = false)]
    pub install_skill: bool,
    #[arg(long = "uninstall-skill", default_value_t = false)]
    pub uninstall_skill: bool,
    #[arg(long = "skill-targets")]
    pub skill_targets: Option<String>,
    #[arg(long = "skill-name")]
    pub skill_name: Option<String>,
    #[arg(long = "force-skill-overwrite", default_value_t = false)]
    pub force_skill_overwrite: bool,
    #[arg(long = "skill-dir-claude")]
    pub skill_dir_claude: Option<String>,
    #[arg(long = "skill-dir-codex")]
    pub skill_dir_codex: Option<String>,
    #[arg(long = "skill-dir-copilot")]
    pub skill_dir_copilot: Option<String>,
    #[arg(long = "skill-dir-opencode")]
    pub skill_dir_opencode: Option<String>,
}

#[derive(Debug, Args)]
pub struct RepairArgs {
    #[arg(long, default_value_t = false)]
    pub fix: bool,
    #[arg(long = "force-unlock", default_value_t = false)]
    pub force_unlock: bool,
}

#[derive(Debug, Args)]
pub struct HistoryArgs {
    pub id: String,
    #[arg(long)]
    pub limit: Option<String>,
    #[arg(long = "type")]
    pub event_type: Option<String>,
    #[arg(long)]
    pub actor: Option<String>,
    #[arg(long)]
    pub since: Option<String>,
}

#[derive(Debug, Args)]
pub struct WatchArgs {
    #[arg(long, default_value = "2")]
    pub interval: String,
    #[arg(long, default_value = "open,in_progress")]
    pub status: String,
    #[arg(long)]
    pub assignee: Option<String>,
    #[arg(long, default_value_t = false)]
    pub tree: bool,
    #[arg(long, default_value_t = false)]
    pub once: bool,
}

pub fn execute_init(service: &TasqueService, args: InitArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq init",
        opts,
        || {
            let options = InitCommandOptions {
                install_skill: args.install_skill,
                uninstall_skill: args.uninstall_skill,
                wizard: args.wizard,
                no_wizard: args.no_wizard,
                yes: args.yes,
                preset: args.preset.clone(),
                skill_targets: args.skill_targets.clone(),
                skill_name: args.skill_name.clone(),
                force_skill_overwrite: args.force_skill_overwrite,
                skill_dir_claude: args.skill_dir_claude.clone(),
                skill_dir_codex: args.skill_dir_codex.clone(),
                skill_dir_copilot: args.skill_dir_copilot.clone(),
                skill_dir_opencode: args.skill_dir_opencode.clone(),
            };
            let raw_args: Vec<String> = std::env::args().skip(1).collect();
            let plan = resolve_init_plan(
                &options,
                &InitResolutionContext {
                    raw_args,
                    is_tty: std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
                    json: opts.json,
                },
            )?;
            match plan {
                InitPlan::NonInteractive { input } => service.init(input),
                InitPlan::Wizard { auto_accept, seed } => {
                    let input = run_init_wizard(seed, auto_accept)?;
                    service.init(input)
                }
            }
        },
        |data| data.clone(),
        |data| {
            for file in &data.files {
                println!("created {}", file);
            }
            if let Some(skill_operation) = &data.skill_operation {
                for result in &skill_operation.results {
                    let message = result
                        .message
                        .as_ref()
                        .map(|value| format!(" {}", value))
                        .unwrap_or_default();
                    println!(
                        "skill {} {} {}{}",
                        skill_target_to_string(result.target),
                        skill_result_status_to_string(result.status),
                        result.path,
                        message
                    );
                }
            }
            Ok(())
        },
    )
}

pub fn execute_doctor(service: &TasqueService, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq doctor",
        opts,
        || service.doctor(),
        |data| data.clone(),
        |data| {
            println!(
                "tasks={} events={} snapshot_loaded={}",
                data.tasks, data.events, data.snapshot_loaded
            );
            if let Some(warning) = &data.warning {
                println!("warning={}", warning);
            }
            if data.issues.is_empty() {
                println!("issues=none");
            } else {
                for issue in &data.issues {
                    println!("issue={}", issue);
                }
            }
            Ok(())
        },
    )
}

pub fn execute_repair(service: &TasqueService, args: RepairArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq repair",
        opts,
        || service.repair(args.fix, args.force_unlock),
        |data| data.clone(),
        |data| {
            print_repair_result(data);
            Ok(())
        },
    )
}

pub fn execute_orphans(service: &TasqueService, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq orphans",
        opts,
        || service.orphans(),
        |data| data.clone(),
        |data| {
            print_orphans_result(data);
            Ok(())
        },
    )
}

pub fn execute_history(service: &TasqueService, args: HistoryArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq history",
        opts,
        || {
            let limit = args
                .limit
                .as_deref()
                .map(|value| parse_positive_int(value, "limit", 1, 10000))
                .transpose()?
                .map(|value| value as usize);
            service.history(HistoryInput {
                id: args.id.clone(),
                limit,
                event_type: args.event_type.clone(),
                actor: args.actor.clone(),
                since: args.since.clone(),
                exact_id: opts.exact_id,
            })
        },
        |data| data.clone(),
        |data| {
            print_history(data);
            Ok(())
        },
    )
}

pub fn execute_watch(service: &TasqueService, args: WatchArgs, opts: GlobalOpts) -> i32 {
    let watch_options = match build_watch_options(args, opts.json) {
        Ok(options) => options,
        Err(error) => {
            if opts.json {
                let envelope = err_envelope(
                    "tsq watch",
                    error.code.clone(),
                    error.message.clone(),
                    error.details.clone(),
                );
                println!(
                    "{}",
                    serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| "{}".to_string())
                );
            } else {
                eprintln!("{}: {}", error.code, error.message);
            }
            return error.exit_code;
        }
    };
    start_watch(service, watch_options)
}

fn build_watch_options(args: WatchArgs, json: bool) -> Result<WatchOptions, TsqError> {
    let interval = parse_positive_int(&args.interval, "interval", 1, 60)?;
    let statuses = parse_status_csv(&args.status)?;
    Ok(WatchOptions {
        interval,
        statuses,
        assignee: as_optional_string(args.assignee.as_deref()),
        tree: args.tree,
        once: args.once,
        json,
    })
}

fn skill_target_to_string(target: crate::skills::types::SkillTarget) -> &'static str {
    match target {
        crate::skills::types::SkillTarget::Claude => "claude",
        crate::skills::types::SkillTarget::Codex => "codex",
        crate::skills::types::SkillTarget::Copilot => "copilot",
        crate::skills::types::SkillTarget::Opencode => "opencode",
    }
}

fn skill_result_status_to_string(status: crate::skills::types::SkillResultStatus) -> &'static str {
    match status {
        crate::skills::types::SkillResultStatus::Installed => "installed",
        crate::skills::types::SkillResultStatus::Updated => "updated",
        crate::skills::types::SkillResultStatus::Skipped => "skipped",
        crate::skills::types::SkillResultStatus::Removed => "removed",
        crate::skills::types::SkillResultStatus::NotFound => "not_found",
    }
}
