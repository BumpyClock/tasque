use crate::app::service::TasqueService;
use crate::app::service_types::{
    SpecAttachInput, SpecCheckInput, SpecContentInput, SpecContentResult, SpecPatchInput,
    SpecUpdateInput, SpecUpdateResult,
};
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::parsers::as_optional_string;
use crate::cli::render::{print_spec_content, print_task};
use crate::errors::TsqError;
use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum SpecCommand {
    Attach(SpecAttachArgs),
    Check(SpecCheckArgs),
}

#[derive(Debug, Args)]
pub struct SpecAttachArgs {
    pub id: String,
    pub source: Option<String>,
    #[arg(long)]
    pub file: Option<String>,
    #[arg(long)]
    pub stdin: bool,
    #[arg(long)]
    pub text: Option<String>,
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct SpecCheckArgs {
    pub id: String,
}

#[derive(Debug, Args)]
#[command(after_help = "Examples:
	  tsq spec tsq-abc12345 --file docs/spec.md
	  tsq spec tsq-abc12345 --update --stdin
	  tsq spec tsq-abc12345 --patch --file spec.patch
	  tsq spec tsq-abc12345 --text '# Context\n...'
	  tsq spec tsq-abc12345 --show
	  tsq spec tsq-abc12345 --check")]
pub struct SpecArgs {
    pub id: String,
    #[arg(long)]
    pub file: Option<String>,
    #[arg(long)]
    pub stdin: bool,
    #[arg(long)]
    pub text: Option<String>,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub update: bool,
    #[arg(long)]
    pub patch: bool,
    #[arg(long)]
    pub show: bool,
    #[arg(long)]
    pub check: bool,
}

pub fn execute_spec(service: &TasqueService, command: SpecCommand, opts: GlobalOpts) -> i32 {
    match command {
        SpecCommand::Attach(args) => run_action(
            "tsq spec attach",
            opts,
            || {
                service.spec_attach(SpecAttachInput {
                    id: args.id.clone(),
                    source: as_optional_string(args.source.as_deref()),
                    file: as_optional_string(args.file.as_deref()),
                    stdin: args.stdin,
                    text: args.text.clone(),
                    force: args.force,
                    exact_id: opts.exact_id,
                })
            },
            |data| data.clone(),
            |data| {
                print_task(&data.task);
                println!("spec={}", data.spec.spec_path);
                println!("spec_sha256={}", data.spec.spec_fingerprint);
                Ok(())
            },
        ),
        SpecCommand::Check(args) => run_action(
            "tsq spec check",
            opts,
            || {
                service.spec_check(SpecCheckInput {
                    id: args.id.clone(),
                    exact_id: opts.exact_id,
                })
            },
            |data| data.clone(),
            |data| {
                println!("task={}", data.task_id);
                println!("spec_ok={}", data.ok);
                if let Some(spec_path) = data.spec.spec_path.as_deref() {
                    println!("spec={}", spec_path);
                }
                if let Some(expected_fingerprint) = data.spec.expected_fingerprint.as_deref() {
                    println!("spec_sha256_expected={}", expected_fingerprint);
                }
                if let Some(actual_fingerprint) = data.spec.actual_fingerprint.as_deref() {
                    println!("spec_sha256_actual={}", actual_fingerprint);
                }
                if !data.spec.missing_sections.is_empty() {
                    println!("missing_sections={}", data.spec.missing_sections.join(","));
                }
                for diagnostic in &data.diagnostics {
                    println!(
                        "diagnostic={}:{}",
                        spec_diagnostic_code_to_string(&diagnostic.code),
                        diagnostic.message
                    );
                }
                Ok(())
            },
        ),
    }
}

pub fn execute_spec_verb(service: &TasqueService, args: SpecArgs, opts: GlobalOpts) -> i32 {
    let action = match classify_spec_action(&args) {
        Ok(action) => action,
        Err(error) => {
            return run_action(
                "tsq spec",
                opts,
                || -> Result<(), TsqError> { Err(error) },
                |_: &()| serde_json::json!({}),
                |_: &()| Ok(()),
            );
        }
    };

    match action {
        SpecAction::Attach => run_action(
            "tsq spec",
            opts,
            || {
                service.spec_attach(SpecAttachInput {
                    id: args.id.clone(),
                    source: None,
                    file: as_optional_string(args.file.as_deref()),
                    stdin: args.stdin,
                    text: args.text.clone(),
                    force: args.force,
                    exact_id: opts.exact_id,
                })
            },
            |data| data.clone(),
            |data| {
                print_task(&data.task);
                println!("spec={}", data.spec.spec_path);
                println!("spec_sha256={}", data.spec.spec_fingerprint);
                Ok(())
            },
        ),
        SpecAction::Show => run_action(
            "tsq spec",
            opts,
            || {
                service.spec_content(SpecContentInput {
                    id: args.id.clone(),
                    exact_id: opts.exact_id,
                })
            },
            spec_content_json,
            |data| {
                print_spec_content(data);
                Ok(())
            },
        ),
        SpecAction::Update => run_action(
            "tsq spec",
            opts,
            || {
                service.spec_update(SpecUpdateInput {
                    id: args.id.clone(),
                    file: as_optional_string(args.file.as_deref()),
                    stdin: args.stdin,
                    text: args.text.clone(),
                    exact_id: opts.exact_id,
                })
            },
            |data| data.clone(),
            |data| {
                print_spec_update_result(data);
                Ok(())
            },
        ),
        SpecAction::Patch => run_action(
            "tsq spec",
            opts,
            || {
                service.spec_patch(SpecPatchInput {
                    id: args.id.clone(),
                    file: as_optional_string(args.file.as_deref()),
                    stdin: args.stdin,
                    text: args.text.clone(),
                    exact_id: opts.exact_id,
                })
            },
            |data| data.clone(),
            |data| {
                print_spec_update_result(data);
                Ok(())
            },
        ),
        SpecAction::Check => run_action(
            "tsq spec",
            opts,
            || {
                service.spec_check(SpecCheckInput {
                    id: args.id.clone(),
                    exact_id: opts.exact_id,
                })
            },
            |data| data.clone(),
            |data| {
                println!("task={}", data.task_id);
                println!("spec_ok={}", data.ok);
                if let Some(spec_path) = data.spec.spec_path.as_deref() {
                    println!("spec={}", spec_path);
                }
                if let Some(expected_fingerprint) = data.spec.expected_fingerprint.as_deref() {
                    println!("spec_sha256_expected={}", expected_fingerprint);
                }
                if let Some(actual_fingerprint) = data.spec.actual_fingerprint.as_deref() {
                    println!("spec_sha256_actual={}", actual_fingerprint);
                }
                if !data.spec.missing_sections.is_empty() {
                    println!("missing_sections={}", data.spec.missing_sections.join(","));
                }
                for diagnostic in &data.diagnostics {
                    println!(
                        "diagnostic={}:{}",
                        spec_diagnostic_code_to_string(&diagnostic.code),
                        diagnostic.message
                    );
                }
                Ok(())
            },
        ),
    }
}

#[derive(Debug, Clone, Copy)]
enum SpecAction {
    Attach,
    Show,
    Update,
    Patch,
    Check,
}

fn classify_spec_action(args: &SpecArgs) -> Result<SpecAction, TsqError> {
    let attach_sources = [
        as_optional_string(args.file.as_deref()).is_some(),
        args.stdin,
        args.text.is_some(),
    ]
    .into_iter()
    .filter(|provided| *provided)
    .count();
    let actions = attach_sources + usize::from(args.show) + usize::from(args.check);
    if args.update && args.patch {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --update with --patch",
            1,
        ));
    }
    if (args.update || args.patch) && attach_sources != 1 {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "--update and --patch require exactly one source: --text, --file, or --stdin",
            1,
        ));
    }
    if (args.update || args.patch) && (args.show || args.check) {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "--update and --patch cannot be combined with --show or --check",
            1,
        ));
    }
    if actions != 1 {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "exactly one spec action is required: --text, --file, --stdin, --update with a source, --patch with a source, --show, or --check",
            1,
        ));
    }
    if args.force && (attach_sources == 0 || args.update || args.patch) {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "--force only applies to spec attach with --text, --file, or --stdin",
            1,
        ));
    }
    if args.show {
        Ok(SpecAction::Show)
    } else if args.check {
        Ok(SpecAction::Check)
    } else if args.update {
        Ok(SpecAction::Update)
    } else if args.patch {
        Ok(SpecAction::Patch)
    } else {
        Ok(SpecAction::Attach)
    }
}

fn spec_content_json(data: &SpecContentResult) -> serde_json::Value {
    serde_json::json!({
        "spec": {
            "path": data.spec_path.as_str(),
            "fingerprint": data.spec_fingerprint.as_str(),
            "content": data.content.as_str(),
        }
    })
}

fn print_spec_update_result(data: &SpecUpdateResult) {
    print_task(&data.task);
    println!("spec={}", data.spec.spec_path);
    println!("spec_sha256_old={}", data.spec.old_fingerprint);
    println!("spec_sha256_new={}", data.spec.new_fingerprint);
}

fn spec_diagnostic_code_to_string(
    code: &crate::app::storage::SpecCheckDiagnosticCode,
) -> &'static str {
    match code {
        crate::app::storage::SpecCheckDiagnosticCode::SpecNotAttached => "SPEC_NOT_ATTACHED",
        crate::app::storage::SpecCheckDiagnosticCode::SpecMetadataInvalid => {
            "SPEC_METADATA_INVALID"
        }
        crate::app::storage::SpecCheckDiagnosticCode::SpecFileMissing => "SPEC_FILE_MISSING",
        crate::app::storage::SpecCheckDiagnosticCode::SpecFingerprintDrift => {
            "SPEC_FINGERPRINT_DRIFT"
        }
        crate::app::storage::SpecCheckDiagnosticCode::SpecRequiredSectionsMissing => {
            "SPEC_REQUIRED_SECTIONS_MISSING"
        }
    }
}
