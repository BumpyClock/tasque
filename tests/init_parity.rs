mod common;

use common::{assert_validation_error, make_repo, run_json};
use tasque::cli::init_flow::{
    InitCommandOptions, InitPlan, InitResolutionContext, resolve_init_plan, run_init_wizard,
};

fn assert_init_validation_error(result: &common::JsonOutput, expected_message: &str) {
    assert_eq!(
        result.cli.code, 1,
        "expected init validation failure\nstdout:\n{}\nstderr:\n{}",
        result.cli.stdout, result.cli.stderr
    );
    assert_eq!(
        result
            .envelope
            .get("command")
            .and_then(|value| value.as_str()),
        Some("tsq init")
    );
    assert_validation_error(result);
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|value| value.get("message"))
            .and_then(|value| value.as_str()),
        Some(expected_message)
    );
}

#[test]
fn init_rejects_invalid_flag_combinations_with_validation_error_envelopes() {
    let repo = make_repo();

    let wizard_conflict = run_json(repo.path(), ["init", "--wizard", "--no-wizard"]);
    assert_init_validation_error(&wizard_conflict, "cannot combine --wizard with --no-wizard");

    let preset_conflict = run_json(repo.path(), ["init", "--preset", "minimal", "--no-wizard"]);
    assert_init_validation_error(&preset_conflict, "cannot combine --preset with --no-wizard");

    let skill_action_conflict = run_json(
        repo.path(),
        ["init", "--install-skill", "--uninstall-skill"],
    );
    assert_init_validation_error(
        &skill_action_conflict,
        "cannot combine --install-skill with --uninstall-skill",
    );
}

#[test]
fn init_rejects_non_interactive_skill_scoped_flags_without_skill_action() {
    let repo = make_repo();
    let expected = "skill options require --install-skill or --uninstall-skill";

    let skill_targets = run_json(repo.path(), ["init", "--skill-targets", "claude,codex"]);
    assert_init_validation_error(&skill_targets, expected);

    let skill_name = run_json(repo.path(), ["init", "--skill-name", "custom-skill"]);
    assert_init_validation_error(&skill_name, expected);

    let force_overwrite = run_json(repo.path(), ["init", "--force-skill-overwrite"]);
    assert_init_validation_error(&force_overwrite, expected);

    let skill_dir = run_json(repo.path(), ["init", "--skill-dir-codex", "./skills/codex"]);
    assert_init_validation_error(&skill_dir, expected);
}

fn assert_resolve_validation_error(err: tasque::TsqError, expected_message: &str) {
    assert_eq!(err.code, "VALIDATION_ERROR");
    assert_eq!(err.exit_code, 1);
    assert_eq!(err.message, expected_message);
}

#[test]
fn resolve_init_plan_rejects_wizard_with_json_in_tty_context() {
    let options = InitCommandOptions {
        wizard: true,
        ..InitCommandOptions::default()
    };
    let context = InitResolutionContext {
        raw_args: vec![
            "init".to_string(),
            "--wizard".to_string(),
            "--json".to_string(),
        ],
        is_tty: true,
        json: true,
    };

    let err = resolve_init_plan(&options, &context).expect_err("expected validation error");
    assert_resolve_validation_error(err, "--wizard is not supported with --json");
}

#[test]
fn resolve_init_plan_rejects_preset_with_json_in_tty_context() {
    let options = InitCommandOptions {
        preset: Some("minimal".to_string()),
        ..InitCommandOptions::default()
    };
    let context = InitResolutionContext {
        raw_args: vec![
            "init".to_string(),
            "--preset".to_string(),
            "minimal".to_string(),
            "--json".to_string(),
        ],
        is_tty: true,
        json: true,
    };

    let err = resolve_init_plan(&options, &context).expect_err("expected validation error");
    assert_resolve_validation_error(err, "--preset is not supported with --json");
}

#[test]
fn resolve_init_plan_preserves_sync_branch_in_non_interactive_input() {
    let options = InitCommandOptions {
        sync_branch: Some("tasque-sync".to_string()),
        ..InitCommandOptions::default()
    };
    let context = InitResolutionContext {
        raw_args: vec!["init".to_string(), "--sync-branch".to_string()],
        is_tty: false,
        json: false,
    };

    let plan = resolve_init_plan(&options, &context).expect("expected init plan");

    match plan {
        InitPlan::NonInteractive { input } => {
            assert_eq!(input.sync_branch.as_deref(), Some("tasque-sync"));
            assert!(!input.install_skill);
            assert!(!input.uninstall_skill);
        }
        InitPlan::Wizard { .. } => panic!("expected non-interactive plan"),
    }
}

#[test]
fn resolve_init_plan_preserves_sync_branch_in_wizard_seed() {
    let options = InitCommandOptions {
        sync_branch: Some("tasque-sync".to_string()),
        ..InitCommandOptions::default()
    };
    let context = InitResolutionContext {
        raw_args: vec!["init".to_string(), "--sync-branch".to_string()],
        is_tty: true,
        json: false,
    };

    let plan = resolve_init_plan(&options, &context).expect("expected init plan");

    match plan {
        InitPlan::Wizard { seed, .. } => {
            assert_eq!(seed.sync_branch.as_deref(), Some("tasque-sync"));
            assert_eq!(seed.action, tasque::cli::init_flow::SkillAction::None);
        }
        InitPlan::NonInteractive { .. } => panic!("expected wizard plan"),
    }
}

#[test]
fn run_init_wizard_auto_accept_preserves_sync_branch_in_input() {
    let options = InitCommandOptions {
        sync_branch: Some("tasque-sync".to_string()),
        ..InitCommandOptions::default()
    };
    let context = InitResolutionContext {
        raw_args: vec!["init".to_string(), "--wizard".to_string()],
        is_tty: true,
        json: false,
    };

    let plan = resolve_init_plan(&options, &context).expect("expected init plan");
    let InitPlan::Wizard { seed, .. } = plan else {
        panic!("expected wizard plan");
    };

    let input = run_init_wizard(seed, true).expect("expected wizard input");

    assert_eq!(input.sync_branch.as_deref(), Some("tasque-sync"));
    assert!(!input.install_skill);
    assert!(!input.uninstall_skill);
}

#[test]
fn sync_branch_does_not_require_skill_action() {
    let options = InitCommandOptions {
        sync_branch: Some("tasque-sync".to_string()),
        ..InitCommandOptions::default()
    };
    let context = InitResolutionContext {
        raw_args: vec!["init".to_string(), "--sync-branch".to_string()],
        is_tty: false,
        json: false,
    };

    let plan = resolve_init_plan(&options, &context).expect("expected init plan");

    match plan {
        InitPlan::NonInteractive { input } => {
            assert_eq!(input.sync_branch.as_deref(), Some("tasque-sync"));
            assert_eq!(input.skill_targets, None);
            assert_eq!(input.skill_name, None);
        }
        InitPlan::Wizard { .. } => panic!("expected non-interactive plan"),
    }
}
