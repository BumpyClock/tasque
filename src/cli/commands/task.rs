use crate::app::runtime::normalize_status;
use crate::app::service::TasqueService;
use crate::app::service_query::ShowResult;
use crate::app::service_types::{
    ClaimInput, DuplicateInput, MergeInput, SpecContentInput, SpecContentResult, StaleInput,
    SupersedeInput, UpdateInput,
};
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::parsers::{
    as_optional_string, parse_non_negative_int, parse_positive_int, parse_priority_value,
};
use crate::cli::render::{
    print_history, print_history_plain, print_merge_result, print_show_result,
    print_show_result_plain, print_spec_content, print_spec_content_plain, print_task,
    print_task_list, print_task_notes, print_task_notes_plain,
};
use crate::errors::TsqError;
use clap::Args;

#[path = "task_create.rs"]
mod task_create;
#[path = "task_find.rs"]
mod task_find;
#[path = "task_lifecycle.rs"]
mod task_lifecycle;

pub use task_create::{CreateArgs, execute_create};
pub use task_find::{FindArgs, execute_find};
pub use task_lifecycle::{
    MultiStatusArgs, NoteStatusArgs, execute_cancel, execute_defer, execute_done, execute_reopen,
};

#[derive(Debug, Args)]
pub struct ShowArgs {
    pub id: String,
    #[arg(long = "with-spec", alias = "spec", default_value_t = false)]
    pub with_spec: bool,
    #[arg(long, default_value_t = false)]
    pub deps: bool,
    #[arg(long, default_value_t = false)]
    pub notes: bool,
    #[arg(long, default_value_t = false)]
    pub history: bool,
    #[arg(long = "all-notes", default_value_t = false)]
    pub all_notes: bool,
    #[arg(long = "all-history", default_value_t = false)]
    pub all_history: bool,
}

#[derive(Debug, Args)]
pub struct StaleArgs {
    #[arg(long, default_value = "30")]
    pub days: String,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub assignee: Option<String>,
    #[arg(long)]
    pub limit: Option<String>,
}

#[derive(Debug, Args)]
pub struct EditArgs {
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
    pub priority: Option<String>,
}

#[derive(Debug, Args)]
pub struct ClaimArgs {
    pub id: String,
    #[arg(long)]
    pub assignee: Option<String>,
    #[arg(long, default_value_t = false)]
    pub start: bool,
    #[arg(long = "require-spec", default_value_t = false)]
    pub require_spec: bool,
}

#[derive(Debug, Args)]
pub struct WorkonArgs {
    pub id: String,
    #[arg(long)]
    pub assignee: Option<String>,
    #[arg(long = "require-spec", default_value_t = false)]
    pub require_spec: bool,
}

#[derive(Debug, Args)]
pub struct AssignArgs {
    pub id: String,
    #[arg(long)]
    pub assignee: String,
}

#[derive(Debug, Args)]
pub struct TaskIdArgs {
    pub id: String,
}

#[derive(Debug, Args)]
pub struct DuplicateArgs {
    pub id: String,
    pub of: String,
    pub canonical: String,
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Debug, Args)]
pub struct DuplicatesArgs {
    #[arg(long, default_value = "20")]
    pub limit: String,
}

#[derive(Debug, Args)]
pub struct SupersedeArgs {
    pub old_id: String,
    pub with: String,
    pub new_id: String,
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Debug, Args)]
pub struct MergeArgs {
    pub sources: Vec<String>,
    #[arg(long = "into")]
    pub into: String,
    #[arg(long)]
    pub reason: Option<String>,
    #[arg(long, default_value_t = false)]
    pub force: bool,
    #[arg(long = "dry-run", default_value_t = false)]
    pub dry_run: bool,
}

pub fn execute_show(service: &TasqueService, args: ShowArgs, opts: GlobalOpts) -> i32 {
    let selected = args.with_spec || args.deps || args.notes || args.history;
    run_action(
        "tsq show",
        opts,
        || {
            let show = service.show(&args.id, opts.exact_id)?;
            let spec_input = SpecContentInput {
                id: args.id.clone(),
                exact_id: opts.exact_id,
            };
            let spec = if args.with_spec {
                Some(service.spec_content(spec_input)).transpose()?
            } else if !selected {
                service.spec_content(spec_input).ok()
            } else {
                None
            };
            Ok((show, spec))
        },
        |(show, spec)| {
            let notes_limit = if args.all_notes { usize::MAX } else { 5 };
            let history_limit = if args.all_history { usize::MAX } else { 10 };
            show_data_json(
                service,
                show,
                spec.as_ref(),
                !selected || args.deps,
                !selected || args.with_spec,
                !selected || args.notes,
                !selected || args.history,
                notes_limit,
                history_limit,
            )
        },
        |(show, spec)| {
            let notes_limit = if args.all_notes { usize::MAX } else { 5 };
            let history_limit = if args.all_history { usize::MAX } else { 10 };
            if opts.plain() {
                if !selected || args.deps {
                    print_show_result_plain(show);
                }
                if let Some(spec) = spec {
                    print_spec_content_plain(spec);
                }
                if !selected || args.notes {
                    let notes = show
                        .task
                        .notes
                        .iter()
                        .take(notes_limit)
                        .cloned()
                        .collect::<Vec<_>>();
                    print_task_notes_plain(&show.task.id, &notes);
                }
                if !selected || args.history {
                    let events = show
                        .history
                        .iter()
                        .rev()
                        .take(history_limit)
                        .cloned()
                        .collect::<Vec<_>>();
                    print_history_plain(&crate::app::service_types::HistoryResult {
                        count: events.len(),
                        truncated: show.history.len() > events.len(),
                        events,
                    });
                }
                return Ok(());
            }
            if !selected || args.deps {
                print_show_result(show);
            }
            if let Some(spec) = spec {
                print_spec_content(spec);
            }
            if !selected || args.notes {
                let notes = show
                    .task
                    .notes
                    .iter()
                    .take(notes_limit)
                    .cloned()
                    .collect::<Vec<_>>();
                print_task_notes(&show.task.id, &notes);
            }
            if !selected || args.history {
                let events = show
                    .history
                    .iter()
                    .rev()
                    .take(history_limit)
                    .cloned()
                    .collect::<Vec<_>>();
                print_history(&crate::app::service_types::HistoryResult {
                    count: events.len(),
                    truncated: show.history.len() > events.len(),
                    events,
                });
            }
            Ok(())
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn show_data_json(
    service: &TasqueService,
    show: &ShowResult,
    spec: Option<&SpecContentResult>,
    include_deps: bool,
    include_spec: bool,
    include_notes: bool,
    include_history: bool,
    notes_limit: usize,
    history_limit: usize,
) -> serde_json::Value {
    let mut json = serde_json::json!({
        "task": show.task,
        "root": service.repo_root(),
    });
    let data = json.as_object_mut().expect("show JSON object");
    if include_deps {
        data.insert(
            "deps".to_string(),
            serde_json::json!({
                "blockers": show.blocker_edges,
                "dependents": show.dependent_edges,
                "ready": show.ready,
                "links": show.links,
            }),
        );
    }
    if include_spec {
        data.insert("spec".to_string(), serde_json::json!(spec));
    }
    if include_notes {
        data.insert(
            "notes".to_string(),
            serde_json::json!(show.task.notes.iter().take(notes_limit).collect::<Vec<_>>()),
        );
    }
    if include_history {
        data.insert(
            "history".to_string(),
            serde_json::json!(
                show.history
                    .iter()
                    .rev()
                    .take(history_limit)
                    .collect::<Vec<_>>()
            ),
        );
    }
    json
}

pub fn execute_stale(service: &TasqueService, args: StaleArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq stale",
        opts,
        || {
            let days = parse_non_negative_int(&args.days, "days")?;
            let status = args.status.as_deref().map(normalize_status).transpose()?;
            let limit = args
                .limit
                .as_deref()
                .map(|value| parse_positive_int(value, "limit", 1, 10000))
                .transpose()?
                .map(|value| value as usize);
            service.stale(&StaleInput {
                days,
                status,
                assignee: as_optional_string(args.assignee.as_deref()),
                limit,
            })
        },
        |data| data.clone(),
        |data| {
            print_task_list(&data.tasks);
            Ok(())
        },
    )
}

pub fn execute_edit(service: &TasqueService, args: EditArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq edit",
        opts,
        || {
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
            service.update(UpdateInput {
                id: args.id.clone(),
                title: as_optional_string(args.title.as_deref()),
                description: as_optional_string(args.description.as_deref()),
                clear_description,
                external_ref: as_optional_string(args.external_ref.as_deref()),
                discovered_from: as_optional_string(args.discovered_from.as_deref()),
                clear_discovered_from,
                clear_external_ref,
                status: None,
                priority: args
                    .priority
                    .as_deref()
                    .map(parse_priority_value)
                    .transpose()?,
                exact_id: opts.exact_id,
                planning_state: None,
                assignee: None,
            })
        },
        |task| serde_json::json!({ "task": task }),
        |task| {
            print_task(task);
            Ok(())
        },
    )
}

pub fn execute_duplicate(service: &TasqueService, args: DuplicateArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq duplicate",
        opts,
        || {
            validate_sentence_token(&args.of, "of", "tsq duplicate <id> of <canonical>")?;
            service.duplicate(DuplicateInput {
                source: args.id.clone(),
                canonical: args.canonical.clone(),
                reason: args.note.clone(),
                exact_id: opts.exact_id,
            })
        },
        |task| serde_json::json!({ "task": task }),
        |task| {
            print_task(task);
            Ok(())
        },
    )
}

pub fn execute_duplicates(service: &TasqueService, args: DuplicatesArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq duplicates",
        opts,
        || {
            let limit = parse_positive_int(&args.limit, "limit", 1, 200)? as usize;
            service.duplicate_candidates(Some(limit))
        },
        |data| data.clone(),
        |data| {
            if data.groups.is_empty() {
                println!("no duplicate candidates");
                return Ok(());
            }
            println!("scanned={} groups={}", data.scanned, data.groups.len());
            for group in &data.groups {
                let ids = group
                    .tasks
                    .iter()
                    .map(|task| task.id.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                println!("{}: {}", group.key, ids);
            }
            Ok(())
        },
    )
}

pub fn execute_supersede(service: &TasqueService, args: SupersedeArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq supersede",
        opts,
        || {
            validate_sentence_token(&args.with, "with", "tsq supersede <old> with <new>")?;
            service.supersede(SupersedeInput {
                source: args.old_id.clone(),
                with_id: args.new_id.clone(),
                reason: args.note.clone(),
                exact_id: opts.exact_id,
            })
        },
        |task| serde_json::json!({ "task": task }),
        |task| {
            print_task(task);
            Ok(())
        },
    )
}

pub fn execute_merge(service: &TasqueService, args: MergeArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq merge",
        opts,
        || {
            service.merge(MergeInput {
                sources: args.sources.clone(),
                into: args.into.clone(),
                reason: args.reason.clone(),
                force: args.force,
                dry_run: args.dry_run,
                exact_id: opts.exact_id,
            })
        },
        |data| data.clone(),
        |data| {
            print_merge_result(data);
            Ok(())
        },
    )
}

pub fn execute_claim(service: &TasqueService, args: ClaimArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq claim",
        opts,
        || {
            let _ = args.start;
            service.claim(ClaimInput {
                id: args.id.clone(),
                assignee: as_optional_string(args.assignee.as_deref()),
                start: args.start,
                require_spec: args.require_spec,
                exact_id: opts.exact_id,
            })
        },
        |task| serde_json::json!({ "task": task }),
        |task| {
            print_task(task);
            Ok(())
        },
    )
}

pub fn execute_workon(service: &TasqueService, args: WorkonArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq workon",
        opts,
        || {
            service.claim(ClaimInput {
                id: args.id.clone(),
                assignee: as_optional_string(args.assignee.as_deref()),
                start: true,
                require_spec: args.require_spec,
                exact_id: opts.exact_id,
            })?;
            let show = service.show(&args.id, opts.exact_id)?;
            let spec_input = SpecContentInput {
                id: args.id.clone(),
                exact_id: opts.exact_id,
            };
            let spec = service.spec_content(spec_input).ok();
            Ok((show, spec))
        },
        |(show, spec)| show_data_json(service, show, spec.as_ref(), true, true, true, true, 5, 10),
        |(show, _spec)| {
            if opts.plain() {
                crate::cli::render::print_show_result_plain(show);
            } else {
                print_show_result(show);
            }
            Ok(())
        },
    )
}

pub fn execute_assign(service: &TasqueService, args: AssignArgs, opts: GlobalOpts) -> i32 {
    run_action(
        "tsq assign",
        opts,
        || {
            service.update(UpdateInput {
                id: args.id.clone(),
                title: None,
                description: None,
                clear_description: false,
                external_ref: None,
                discovered_from: None,
                clear_discovered_from: false,
                clear_external_ref: false,
                status: None,
                priority: None,
                exact_id: opts.exact_id,
                planning_state: None,
                assignee: as_optional_string(Some(&args.assignee)),
            })
        },
        |task| serde_json::json!({ "task": task }),
        |task| {
            print_task(task);
            Ok(())
        },
    )
}

pub fn execute_set_status(
    service: &TasqueService,
    args: TaskIdArgs,
    status: crate::types::TaskStatus,
    command_line: &'static str,
    opts: GlobalOpts,
) -> i32 {
    run_action(
        command_line,
        opts,
        || {
            service.update(UpdateInput {
                id: args.id.clone(),
                title: None,
                description: None,
                clear_description: false,
                external_ref: None,
                discovered_from: None,
                clear_discovered_from: false,
                clear_external_ref: false,
                status: Some(status),
                priority: None,
                exact_id: opts.exact_id,
                planning_state: None,
                assignee: None,
            })
        },
        |task| serde_json::json!({ "task": task }),
        |task| {
            print_task(task);
            Ok(())
        },
    )
}

pub fn execute_set_planning(
    service: &TasqueService,
    args: TaskIdArgs,
    planning_state: crate::types::PlanningState,
    command_line: &'static str,
    opts: GlobalOpts,
) -> i32 {
    run_action(
        command_line,
        opts,
        || {
            service.update(UpdateInput {
                id: args.id.clone(),
                title: None,
                description: None,
                clear_description: false,
                external_ref: None,
                discovered_from: None,
                clear_discovered_from: false,
                clear_external_ref: false,
                status: None,
                priority: None,
                exact_id: opts.exact_id,
                planning_state: Some(planning_state),
                assignee: None,
            })
        },
        |task| serde_json::json!({ "task": task }),
        |task| {
            print_task(task);
            Ok(())
        },
    )
}

fn validate_sentence_token(value: &str, expected: &str, example: &str) -> Result<(), TsqError> {
    if value == expected {
        return Ok(());
    }
    Err(TsqError::new(
        "VALIDATION_ERROR",
        format!("expected `{}`; use `{}`", expected, example),
        1,
    ))
}
