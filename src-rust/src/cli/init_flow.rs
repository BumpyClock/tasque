use crate::app::service_types::InitInput;
use crate::cli::parsers::{InitPreset, as_optional_string, parse_init_preset, parse_skill_targets};
use crate::errors::TsqError;
use crate::skills::types::SkillTarget;
use std::io::{self, Write};

const ALL_SKILL_TARGETS: [SkillTarget; 4] = [
    SkillTarget::Claude,
    SkillTarget::Codex,
    SkillTarget::Copilot,
    SkillTarget::Opencode,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillAction {
    None,
    Install,
    Uninstall,
}

#[derive(Debug, Clone)]
pub struct InitResolutionContext {
    pub raw_args: Vec<String>,
    pub is_tty: bool,
    pub json: bool,
}

#[derive(Debug, Clone, Default)]
pub struct InitCommandOptions {
    pub install_skill: bool,
    pub uninstall_skill: bool,
    pub wizard: bool,
    pub no_wizard: bool,
    pub yes: bool,
    pub preset: Option<String>,
    pub skill_targets: Option<String>,
    pub skill_name: Option<String>,
    pub force_skill_overwrite: bool,
    pub skill_dir_claude: Option<String>,
    pub skill_dir_codex: Option<String>,
    pub skill_dir_copilot: Option<String>,
    pub skill_dir_opencode: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WizardSeed {
    pub action: SkillAction,
    pub skill_targets: Vec<SkillTarget>,
    pub skill_name: String,
    pub force_skill_overwrite: bool,
    pub skill_dir_claude: Option<String>,
    pub skill_dir_codex: Option<String>,
    pub skill_dir_copilot: Option<String>,
    pub skill_dir_opencode: Option<String>,
}

#[derive(Debug, Clone)]
pub enum InitPlan {
    NonInteractive { input: InitInput },
    Wizard { auto_accept: bool, seed: WizardSeed },
}

pub fn resolve_init_plan(
    options: &InitCommandOptions,
    context: &InitResolutionContext,
) -> Result<InitPlan, TsqError> {
    let has_wizard = has_flag(&context.raw_args, "--wizard");
    let has_no_wizard = has_flag(&context.raw_args, "--no-wizard");
    let preset = options
        .preset
        .as_deref()
        .map(parse_init_preset)
        .transpose()?;

    if has_wizard && has_no_wizard {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --wizard with --no-wizard",
            1,
        ));
    }
    if preset.is_some() && has_no_wizard {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --preset with --no-wizard",
            1,
        ));
    }
    if options.install_skill && options.uninstall_skill {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --install-skill with --uninstall-skill",
            1,
        ));
    }
    if !context.is_tty && has_wizard {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "--wizard requires an interactive TTY",
            1,
        ));
    }
    if !context.is_tty && preset.is_some() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "--preset requires an interactive TTY",
            1,
        ));
    }
    if context.json && has_wizard {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "--wizard is not supported with --json",
            1,
        ));
    }
    if context.json && preset.is_some() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "--preset is not supported with --json",
            1,
        ));
    }

    let has_explicit_skill_action = options.install_skill || options.uninstall_skill;
    let wizard_enabled = !has_no_wizard
        && (has_wizard || (!has_explicit_skill_action && context.is_tty && !context.json));
    if !wizard_enabled {
        return Ok(InitPlan::NonInteractive {
            input: resolve_non_interactive_input(options)?,
        });
    }

    Ok(InitPlan::Wizard {
        auto_accept: options.yes,
        seed: resolve_wizard_seed(options, preset)?,
    })
}

pub fn run_init_wizard(seed: WizardSeed, auto_accept: bool) -> Result<InitInput, TsqError> {
    let mut state = seed.clone();
    if auto_accept {
        print_header(4, 4);
        print_plan_summary(&state);
        println!("\n--yes enabled: applying defaults and confirmation automatically.");
        return Ok(build_init_input_from_seed(&state));
    }

    let mut step = 1;
    loop {
        if step == 1 {
            print_header(1, 4);
            println!(
                "This will initialize: .tasque/config.json, .tasque/events.jsonl, .tasque/.gitignore"
            );
            let decision = ask_token("Continue setup? [Y/n/s/q] ")?;
            if is_yes(&decision) {
                step = 2;
                continue;
            }
            if decision == "s" {
                step = 4;
                continue;
            }
            if is_no(&decision) || decision == "q" {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "init canceled by user",
                    1,
                ));
            }
            print_invalid();
            continue;
        }

        if step == 2 {
            print_header(2, 4);
            println!("Select skill action:");
            println!("  1) install");
            println!("  2) uninstall");
            println!("  3) none");
            let default_choice = default_action_choice(state.action);
            let answer = ask_token(&format!(
                "Skill action [1/2/3] (default {}, b=back, s=skip, q=quit) ",
                default_choice
            ))?;
            if answer == "b" {
                step = 1;
                continue;
            }
            if answer == "s" {
                step = 4;
                continue;
            }
            if answer == "q" {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "init canceled by user",
                    1,
                ));
            }
            let normalized = if answer.is_empty() {
                default_choice.to_string()
            } else {
                answer
            };
            match normalized.as_str() {
                "1" | "install" => {
                    state.action = SkillAction::Install;
                    step = 3;
                }
                "2" | "uninstall" => {
                    state.action = SkillAction::Uninstall;
                    step = 3;
                }
                "3" | "none" => {
                    state.action = SkillAction::None;
                    step = 4;
                }
                _ => print_invalid(),
            }
            continue;
        }

        if step == 3 {
            if state.action == SkillAction::None {
                step = 4;
                continue;
            }
            print_header(3, 4);
            let default_targets = format_targets(&state.skill_targets);
            let targets_answer = ask_token(&format!(
                "Skill targets [all or csv] (default {}, b=back, s=skip, q=quit) ",
                default_targets
            ))?;
            if targets_answer == "b" {
                step = 2;
                continue;
            }
            if targets_answer == "s" {
                step = 4;
                continue;
            }
            if targets_answer == "q" {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "init canceled by user",
                    1,
                ));
            }
            if !targets_answer.is_empty() {
                match parse_skill_targets(&targets_answer) {
                    Ok(targets) => state.skill_targets = targets,
                    Err(error) => {
                        println!("{}: {}", error.code, error.message);
                        continue;
                    }
                }
            }

            let name_answer = ask_token(&format!(
                "Skill name (default {}, b=back, s=skip, q=quit) ",
                state.skill_name
            ))?;
            if name_answer == "b" {
                step = 2;
                continue;
            }
            if name_answer == "s" {
                step = 4;
                continue;
            }
            if name_answer == "q" {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "init canceled by user",
                    1,
                ));
            }
            if !name_answer.is_empty() {
                state.skill_name = name_answer;
            }

            if state.action == SkillAction::Install {
                let force_answer = ask_token(&format!(
                    "Force overwrite unmanaged skill dirs? [y/N] (default {}) ",
                    if state.force_skill_overwrite {
                        "y"
                    } else {
                        "n"
                    }
                ))?;
                if force_answer == "q" {
                    return Err(TsqError::new(
                        "VALIDATION_ERROR",
                        "init canceled by user",
                        1,
                    ));
                }
                if force_answer == "b" {
                    step = 2;
                    continue;
                }
                if force_answer == "s" {
                    step = 4;
                    continue;
                }
                if is_yes(&force_answer) {
                    state.force_skill_overwrite = true;
                } else if is_no(&force_answer) {
                    state.force_skill_overwrite = false;
                }
            }

            step = 4;
            continue;
        }

        print_header(4, 4);
        print_plan_summary(&state);
        let confirm = ask_token("Apply this setup? [Y/n/b/s/q] ")?;
        if is_yes(&confirm) || confirm == "s" {
            return Ok(build_init_input_from_seed(&state));
        }
        if confirm == "b" {
            step = if state.action == SkillAction::None {
                2
            } else {
                3
            };
            continue;
        }
        if is_no(&confirm) || confirm == "q" {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "init canceled by user",
                1,
            ));
        }
        print_invalid();
    }
}

fn resolve_non_interactive_input(options: &InitCommandOptions) -> Result<InitInput, TsqError> {
    let has_skill_operation = options.install_skill || options.uninstall_skill;
    if !has_skill_operation && has_skill_scoped_flags(options) {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "skill options require --install-skill or --uninstall-skill",
            1,
        ));
    }

    Ok(InitInput {
        install_skill: options.install_skill,
        uninstall_skill: options.uninstall_skill,
        skill_targets: if has_skill_operation {
            Some(parse_skill_targets(
                options.skill_targets.as_deref().unwrap_or("all"),
            )?)
        } else {
            None
        },
        skill_name: if has_skill_operation {
            Some(
                as_optional_string(options.skill_name.as_deref())
                    .unwrap_or_else(|| "tasque".to_string()),
            )
        } else {
            None
        },
        force_skill_overwrite: options.force_skill_overwrite,
        skill_dir_claude: as_optional_string(options.skill_dir_claude.as_deref()),
        skill_dir_codex: as_optional_string(options.skill_dir_codex.as_deref()),
        skill_dir_copilot: as_optional_string(options.skill_dir_copilot.as_deref()),
        skill_dir_opencode: as_optional_string(options.skill_dir_opencode.as_deref()),
    })
}

fn resolve_wizard_seed(
    options: &InitCommandOptions,
    preset: Option<InitPreset>,
) -> Result<WizardSeed, TsqError> {
    let defaults = resolve_preset_defaults(preset);
    let explicit_action = if options.install_skill {
        Some(SkillAction::Install)
    } else if options.uninstall_skill {
        Some(SkillAction::Uninstall)
    } else {
        None
    };

    let action = explicit_action.unwrap_or(defaults.action);
    let skill_targets = if let Some(raw) = options.skill_targets.as_deref() {
        parse_skill_targets(raw)?
    } else {
        defaults.skill_targets.clone()
    };
    let skill_name =
        as_optional_string(options.skill_name.as_deref()).unwrap_or(defaults.skill_name);
    let force_skill_overwrite = options.force_skill_overwrite || defaults.force_skill_overwrite;

    if action == SkillAction::None && has_skill_scoped_flags(options) {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "skill options require --install-skill or --uninstall-skill (or preset that enables skill action)",
            1,
        ));
    }

    Ok(WizardSeed {
        action,
        skill_targets,
        skill_name,
        force_skill_overwrite,
        skill_dir_claude: as_optional_string(options.skill_dir_claude.as_deref()),
        skill_dir_codex: as_optional_string(options.skill_dir_codex.as_deref()),
        skill_dir_copilot: as_optional_string(options.skill_dir_copilot.as_deref()),
        skill_dir_opencode: as_optional_string(options.skill_dir_opencode.as_deref()),
    })
}

fn resolve_preset_defaults(preset: Option<InitPreset>) -> WizardSeed {
    match preset {
        Some(InitPreset::Standard) => WizardSeed {
            action: SkillAction::Install,
            skill_targets: ALL_SKILL_TARGETS.to_vec(),
            skill_name: "tasque".to_string(),
            force_skill_overwrite: false,
            skill_dir_claude: None,
            skill_dir_codex: None,
            skill_dir_copilot: None,
            skill_dir_opencode: None,
        },
        Some(InitPreset::Full) => WizardSeed {
            action: SkillAction::Install,
            skill_targets: ALL_SKILL_TARGETS.to_vec(),
            skill_name: "tasque".to_string(),
            force_skill_overwrite: true,
            skill_dir_claude: None,
            skill_dir_codex: None,
            skill_dir_copilot: None,
            skill_dir_opencode: None,
        },
        _ => WizardSeed {
            action: SkillAction::None,
            skill_targets: ALL_SKILL_TARGETS.to_vec(),
            skill_name: "tasque".to_string(),
            force_skill_overwrite: false,
            skill_dir_claude: None,
            skill_dir_codex: None,
            skill_dir_copilot: None,
            skill_dir_opencode: None,
        },
    }
}

fn build_init_input_from_seed(seed: &WizardSeed) -> InitInput {
    let skill_enabled = seed.action != SkillAction::None;
    InitInput {
        install_skill: seed.action == SkillAction::Install,
        uninstall_skill: seed.action == SkillAction::Uninstall,
        skill_targets: if skill_enabled {
            Some(seed.skill_targets.clone())
        } else {
            None
        },
        skill_name: if skill_enabled {
            Some(seed.skill_name.clone())
        } else {
            None
        },
        force_skill_overwrite: if seed.action == SkillAction::Install {
            seed.force_skill_overwrite
        } else {
            false
        },
        skill_dir_claude: seed.skill_dir_claude.clone(),
        skill_dir_codex: seed.skill_dir_codex.clone(),
        skill_dir_copilot: seed.skill_dir_copilot.clone(),
        skill_dir_opencode: seed.skill_dir_opencode.clone(),
    }
}

fn has_skill_scoped_flags(options: &InitCommandOptions) -> bool {
    options.skill_targets.is_some()
        || options.skill_name.is_some()
        || options.force_skill_overwrite
        || as_optional_string(options.skill_dir_claude.as_deref()).is_some()
        || as_optional_string(options.skill_dir_codex.as_deref()).is_some()
        || as_optional_string(options.skill_dir_copilot.as_deref()).is_some()
        || as_optional_string(options.skill_dir_opencode.as_deref()).is_some()
}

fn has_flag(raw_args: &[String], flag: &str) -> bool {
    raw_args.iter().any(|arg| arg == flag)
}

fn print_header(step: usize, total: usize) {
    println!("\ntsq init wizard [step {}/{}]", step, total);
}

fn print_plan_summary(seed: &WizardSeed) {
    println!("Planned changes:");
    println!("- create .tasque/config.json");
    println!("- create .tasque/events.jsonl");
    println!("- create .tasque/.gitignore");
    match seed.action {
        SkillAction::Install => {
            let force = if seed.force_skill_overwrite {
                " (force overwrite)"
            } else {
                ""
            };
            println!(
                "- install skill \"{}\" to targets: {}{}",
                seed.skill_name,
                format_targets(&seed.skill_targets),
                force
            );
        }
        SkillAction::Uninstall => println!(
            "- uninstall skill \"{}\" from targets: {}",
            seed.skill_name,
            format_targets(&seed.skill_targets)
        ),
        SkillAction::None => println!("- no skill operation"),
    }
}

fn ask_token(question: &str) -> Result<String, TsqError> {
    print!("{}", question.trim());
    print!(" ");
    io::stdout().flush().map_err(|error| {
        TsqError::new("IO_ERROR", "failed writing wizard prompt", 2)
            .with_details(io_error_value(&error))
    })?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer).map_err(|error| {
        TsqError::new("IO_ERROR", "failed reading wizard input", 2)
            .with_details(io_error_value(&error))
    })?;
    Ok(answer.trim().to_lowercase())
}

fn default_action_choice(action: SkillAction) -> &'static str {
    match action {
        SkillAction::Install => "1",
        SkillAction::Uninstall => "2",
        SkillAction::None => "3",
    }
}

fn format_targets(targets: &[SkillTarget]) -> String {
    let mut sorted_targets = targets.to_vec();
    sorted_targets.sort_by_key(|value| target_rank(*value));
    if sorted_targets == ALL_SKILL_TARGETS {
        return "all".to_string();
    }
    sorted_targets
        .iter()
        .map(|target| match target {
            SkillTarget::Claude => "claude",
            SkillTarget::Codex => "codex",
            SkillTarget::Copilot => "copilot",
            SkillTarget::Opencode => "opencode",
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn target_rank(target: SkillTarget) -> usize {
    match target {
        SkillTarget::Claude => 0,
        SkillTarget::Codex => 1,
        SkillTarget::Copilot => 2,
        SkillTarget::Opencode => 3,
    }
}

fn is_yes(value: &str) -> bool {
    value.is_empty() || value == "y" || value == "yes"
}

fn is_no(value: &str) -> bool {
    value == "n" || value == "no"
}

fn print_invalid() {
    println!("Invalid input. Use the shown options.");
}

fn io_error_value(error: &std::io::Error) -> serde_json::Value {
    serde_json::json!({"kind": format!("{:?}", error.kind()), "message": error.to_string()})
}
