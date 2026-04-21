use crate::app::runtime::normalize_status;
use crate::app::service::TasqueService;
use crate::app::service_types::{ClaimInput, UpdateInput};
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::parsers::{as_optional_string, parse_planning_state, parse_priority_value};
use crate::cli::render::print_task;
use crate::errors::TsqError;
use clap::Args;

#[derive(Debug, Args)]
pub struct UpdateArgs {
    pub id: String,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long = "clear-description", default_value_t = false)]
    pub clear_description: bool,
    #[arg(long = "external-ref")]
    pub external_ref: Option<String>,
    #[arg(long = "discovered-from")]
    pub discovered_from: Option<String>,
    #[arg(long = "clear-discovered-from", default_value_t = false)]
    pub clear_discovered_from: bool,
    #[arg(long = "clear-external-ref", default_value_t = false)]
    pub clear_external_ref: bool,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub priority: Option<String>,
    #[arg(long, default_value_t = false)]
    pub claim: bool,
    #[arg(long)]
    pub assignee: Option<String>,
    #[arg(long = "require-spec", default_value_t = false)]
    pub require_spec: bool,
    #[arg(long = "planning")]
    pub planning: Option<String>,
}

pub fn execute_update(service: &TasqueService, args: UpdateArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq update",
        opts,
        || {
            let claim = args.claim;
            let require_spec = args.require_spec;
            let has_description = as_optional_string(args.description.as_deref()).is_some();
            let clear_description = args.clear_description;
            let has_external_ref = as_optional_string(args.external_ref.as_deref()).is_some();
            let has_discovered_from = as_optional_string(args.discovered_from.as_deref()).is_some();
            let clear_external_ref = args.clear_external_ref;
            let clear_discovered_from = args.clear_discovered_from;

            if has_description && clear_description {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "cannot combine --description with --clear-description",
                    1,
                ));
            }
            if has_external_ref && clear_external_ref {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "cannot combine --external-ref with --clear-external-ref",
                    1,
                ));
            }
            if has_discovered_from && clear_discovered_from {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "cannot combine --discovered-from with --clear-discovered-from",
                    1,
                ));
            }
            if !claim && require_spec {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "--require-spec requires --claim",
                    1,
                ));
            }
            if !claim && args.assignee.is_some() {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "--assignee requires --claim",
                    1,
                ));
            }

            if claim {
                if args.title.is_some()
                    || args.status.is_some()
                    || args.priority.is_some()
                    || has_description
                    || clear_description
                    || has_external_ref
                    || clear_external_ref
                    || has_discovered_from
                    || clear_discovered_from
                    || args.planning.is_some()
                {
                    return Err(TsqError::new(
                        "VALIDATION_ERROR",
                        "cannot combine --claim with --title/--description/--clear-description/--external-ref/--clear-external-ref/--discovered-from/--clear-discovered-from/--status/--priority/--planning",
                        1,
                    ));
                }
                return service.claim(ClaimInput {
                    id: args.id.clone(),
                    assignee: as_optional_string(args.assignee.as_deref()),
                    require_spec,
                    exact_id: opts.exact_id,
                });
            }

            service.update(UpdateInput {
                id: args.id.clone(),
                title: as_optional_string(args.title.as_deref()),
                description: as_optional_string(args.description.as_deref()),
                clear_description,
                external_ref: as_optional_string(args.external_ref.as_deref()),
                discovered_from: as_optional_string(args.discovered_from.as_deref()),
                clear_discovered_from,
                clear_external_ref,
                status: args.status.as_deref().map(normalize_status).transpose()?,
                priority: args
                    .priority
                    .as_deref()
                    .map(parse_priority_value)
                    .transpose()?,
                exact_id: opts.exact_id,
                planning_state: args
                    .planning
                    .as_deref()
                    .map(parse_planning_state)
                    .transpose()?,
            })
        },
        |task| serde_json::json!({ "task": task }),
        |task| {
            print_task(task);
            Ok(())
        },
    )
}
