use crate::app::service::TasqueService;
use crate::app::service_types::{SpecAttachInput, SpecCheckInput};
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::parsers::as_optional_string;
use crate::cli::render::print_task;
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
