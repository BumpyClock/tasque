use crate::app::runtime::find_tasque_root;
use crate::app::service::TasqueService;
use crate::cli::action::{GlobalOpts, OutputFormat, emit_error};
use crate::cli::commands::{dep, hooks, label, link, meta, note, skills, spec, sync, task};
use crate::errors::TsqError;
use crate::output::err_envelope;
use clap::error::ErrorKind;
use clap::{Parser, Subcommand, ValueEnum};
use std::io::IsTerminal;

#[derive(Debug, Parser)]
#[command(name = "tsq")]
#[command(version)]
#[command(about = "Local durable task graph for coding agents")]
#[command(after_help = "Examples:
  tsq create \"Fix auth redirect\"
  tsq create --from-file tasks.md
  tsq find ready --lane coding
  tsq note tsq-abc12345 \"blocked on API decision\"
  tsq spec tsq-abc12345 --file docs/spec.md
  tsq done tsq-abc12345 --note \"merged\"")]
pub struct Cli {
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true, value_enum)]
    pub format: Option<FormatArg>,
    #[arg(long = "exact-id", global = true)]
    pub exact_id: bool,
    #[command(subcommand)]
    pub command: CommandKind,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum FormatArg {
    Human,
    Json,
}

#[derive(Debug, Subcommand)]
pub enum CommandKind {
    Init(meta::InitArgs),
    Doctor,
    Repair(meta::RepairArgs),
    Orphans,
    History(meta::HistoryArgs),
    Watch(meta::WatchArgs),
    Tui(meta::TuiArgs),
    Create(task::CreateArgs),
    Show(task::ShowArgs),
    Find(task::FindArgs),
    Stale(task::StaleArgs),
    Edit(task::EditArgs),
    Claim(task::ClaimArgs),
    Assign(task::AssignArgs),
    Start(task::TaskIdArgs),
    Open(task::TaskIdArgs),
    Blocked(task::TaskIdArgs),
    Planned(task::TaskIdArgs),
    NeedsPlan(task::TaskIdArgs),
    Defer(task::NoteStatusArgs),
    Done(task::MultiStatusArgs),
    Duplicate(task::DuplicateArgs),
    Duplicates(task::DuplicatesArgs),
    Supersede(task::SupersedeArgs),
    Merge(task::MergeArgs),
    Reopen(task::MultiStatusArgs),
    Cancel(task::MultiStatusArgs),
    Block(dep::BlockArgs),
    Unblock(dep::UnblockArgs),
    Order(dep::OrderArgs),
    Unorder(dep::UnorderArgs),
    Deps(dep::DepsArgs),
    Relate(link::RelateArgs),
    Unrelate(link::UnrelateArgs),
    Label(label::LabelArgs),
    Unlabel(label::UnlabelArgs),
    Labels,
    Note(note::NoteArgs),
    Notes(note::NoteListArgs),
    Spec(spec::SpecArgs),
    Sync(sync::SyncArgs),
    Hooks {
        #[command(subcommand)]
        command: hooks::HooksCommand,
    },
    /// Manage skills across AI coding targets
    Skills {
        #[command(subcommand)]
        command: skills::SkillsCommand,
    },
    /// Migrate existing events into a sync branch
    Migrate(sync::MigrateArgs),
    /// JSONL merge driver for git (invoked as: tsq merge-driver %O %A %B)
    MergeDriver(sync::MergeDriverArgs),
}

pub fn run_cli(service: &TasqueService) -> i32 {
    let raw_args: Vec<String> = std::env::args_os()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect();
    if let Some(hint) = removed_command_hint(&raw_args) {
        let opts = parse_global_opts_from_args(&raw_args);
        return emit_error("tsq", opts, TsqError::new("VALIDATION_ERROR", hint, 1));
    }

    if std::env::args_os().count() == 1
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
    {
        return execute_command(
            service,
            CommandKind::Tui(meta::TuiArgs::default()),
            GlobalOpts {
                json: false,
                exact_id: false,
            },
        );
    }

    let cli = match Cli::try_parse() {
        Ok(parsed) => parsed,
        Err(error) => return handle_parse_error(service, error),
    };
    let opts = match global_opts(cli.json, cli.format, cli.exact_id) {
        Ok(opts) => opts,
        Err(error) => {
            let fallback_opts = GlobalOpts {
                json: true,
                exact_id: cli.exact_id,
            };
            return emit_error("tsq", fallback_opts, error);
        }
    };
    execute_command(service, cli.command, opts)
}

fn execute_command(service: &TasqueService, command: CommandKind, opts: GlobalOpts) -> i32 {
    if !is_init_safe_command(&command) && find_tasque_root().is_none() {
        let code = "NOT_INITIALIZED";
        let message = "No .tasque directory found. Run 'tsq init' first.";
        if opts.json() {
            let command_line = format!("tsq {}", root_command_name(&command));
            let envelope = err_envelope(
                command_line,
                code,
                message,
                Option::<serde_json::Value>::None,
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| "{}".to_string())
            );
        } else {
            eprintln!("{}: {}", code, message);
        }
        return 2;
    }

    match command {
        CommandKind::Init(args) => meta::execute_init(service, args, opts),
        CommandKind::Doctor => meta::execute_doctor(service, opts),
        CommandKind::Repair(args) => meta::execute_repair(service, args, opts),
        CommandKind::Orphans => meta::execute_orphans(service, opts),
        CommandKind::History(args) => meta::execute_history(service, args, opts),
        CommandKind::Watch(args) => meta::execute_watch(service, args, opts),
        CommandKind::Tui(args) => meta::execute_tui(service, args, opts),
        CommandKind::Create(args) => task::execute_create(service, args, opts),
        CommandKind::Show(args) => task::execute_show(service, args, opts),
        CommandKind::Find(args) => task::execute_find(service, args, opts),
        CommandKind::Stale(args) => task::execute_stale(service, args, opts),
        CommandKind::Edit(args) => task::execute_edit(service, args, opts),
        CommandKind::Claim(args) => task::execute_claim(service, args, opts),
        CommandKind::Assign(args) => task::execute_assign(service, args, opts),
        CommandKind::Start(args) => task::execute_set_status(
            service,
            args,
            crate::types::TaskStatus::InProgress,
            "tsq start",
            opts,
        ),
        CommandKind::Open(args) => task::execute_set_status(
            service,
            args,
            crate::types::TaskStatus::Open,
            "tsq open",
            opts,
        ),
        CommandKind::Blocked(args) => task::execute_set_status(
            service,
            args,
            crate::types::TaskStatus::Blocked,
            "tsq blocked",
            opts,
        ),
        CommandKind::Planned(args) => task::execute_set_planning(
            service,
            args,
            crate::types::PlanningState::Planned,
            "tsq planned",
            opts,
        ),
        CommandKind::NeedsPlan(args) => task::execute_set_planning(
            service,
            args,
            crate::types::PlanningState::NeedsPlanning,
            "tsq needs-plan",
            opts,
        ),
        CommandKind::Defer(args) => task::execute_defer(service, args, opts),
        CommandKind::Done(args) => task::execute_done(service, args, opts),
        CommandKind::Duplicate(args) => task::execute_duplicate(service, args, opts),
        CommandKind::Duplicates(args) => task::execute_duplicates(service, args, opts),
        CommandKind::Supersede(args) => task::execute_supersede(service, args, opts),
        CommandKind::Merge(args) => task::execute_merge(service, args, opts),
        CommandKind::Reopen(args) => task::execute_reopen(service, args, opts),
        CommandKind::Cancel(args) => task::execute_cancel(service, args, opts),
        CommandKind::Block(args) => dep::execute_block(service, args, opts),
        CommandKind::Unblock(args) => dep::execute_unblock(service, args, opts),
        CommandKind::Order(args) => dep::execute_order(service, args, opts),
        CommandKind::Unorder(args) => dep::execute_unorder(service, args, opts),
        CommandKind::Deps(args) => dep::execute_deps(service, args, opts),
        CommandKind::Relate(args) => link::execute_relate(service, args, opts),
        CommandKind::Unrelate(args) => link::execute_unrelate(service, args, opts),
        CommandKind::Label(args) => label::execute_label_add(service, args, opts),
        CommandKind::Unlabel(args) => label::execute_unlabel(service, args, opts),
        CommandKind::Labels => label::execute_labels(service, opts),
        CommandKind::Note(args) => note::execute_note_verb(service, args, opts),
        CommandKind::Notes(args) => note::execute_notes_verb(service, args, opts),
        CommandKind::Spec(args) => spec::execute_spec_verb(service, args, opts),
        CommandKind::Sync(args) => sync::execute_sync(service, args, opts),
        CommandKind::Hooks { command } => hooks::execute_hooks(service, command, opts),
        CommandKind::Skills { command } => skills::execute_skills(service, command, opts),
        CommandKind::Migrate(args) => sync::execute_migrate(service, args, opts),
        CommandKind::MergeDriver(args) => sync::execute_merge_driver(args),
    }
}

fn handle_parse_error(service: &TasqueService, error: clap::Error) -> i32 {
    if error.kind() == ErrorKind::DisplayHelp || error.kind() == ErrorKind::DisplayVersion {
        let _ = error.print();
        return 0;
    }

    if is_missing_subcommand_error(error.kind()) {
        let opts = parse_global_opts_from_env();
        if opts.json() {
            let envelope = err_envelope(
                "tsq",
                "VALIDATION_ERROR",
                "command is required when using --json",
                Option::<serde_json::Value>::None,
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| "{}".to_string())
            );
            return 1;
        }
        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            return execute_command(service, CommandKind::Tui(meta::TuiArgs::default()), opts);
        }
    }

    let exit_code = clap_error_exit_code(error.kind());
    let opts = parse_global_opts_from_env();
    if opts.json() {
        let envelope = err_envelope(
            "tsq",
            "VALIDATION_ERROR",
            error.to_string().trim(),
            Option::<serde_json::Value>::None,
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| "{}".to_string())
        );
        return exit_code;
    }
    let _ = error.print();
    exit_code
}

fn parse_global_opts_from_env() -> GlobalOpts {
    let args: Vec<String> = std::env::args().collect();
    parse_global_opts_from_args(&args)
}

fn parse_global_opts_from_args(args: &[String]) -> GlobalOpts {
    let mut json = false;
    let mut exact_id = false;
    let mut format = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--exact-id" => exact_id = true,
            "--format" => {
                format = match iter.next().map(String::as_str) {
                    Some("json") => Some(FormatArg::Json),
                    Some("human") => Some(FormatArg::Human),
                    _ => format,
                };
            }
            value if value.starts_with("--format=") => {
                format = match value.strip_prefix("--format=") {
                    Some("json") => Some(FormatArg::Json),
                    Some("human") => Some(FormatArg::Human),
                    _ => format,
                };
            }
            _ => {}
        }
    }
    global_opts(json, format, exact_id).unwrap_or(GlobalOpts { json, exact_id })
}

fn global_opts(
    json: bool,
    format: Option<FormatArg>,
    exact_id: bool,
) -> Result<GlobalOpts, TsqError> {
    if json && matches!(format, Some(FormatArg::Human)) {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --json with --format human",
            1,
        ));
    }
    let format = if json || matches!(format, Some(FormatArg::Json)) {
        OutputFormat::Json
    } else {
        OutputFormat::Human
    };
    Ok(GlobalOpts {
        json: json || matches!(format, OutputFormat::Json),
        exact_id,
    })
}

fn removed_command_hint(args: &[String]) -> Option<&'static str> {
    let (root_index, root) = first_command_token(args)?;
    match root {
        "list" => Some("use `tsq find open` or `tsq find <status>`"),
        "ready" => Some("use `tsq find ready --lane coding`"),
        "search" => Some("use `tsq find search \"query\"`"),
        "update" => Some("use `tsq edit <id> ...` or lifecycle verbs like `tsq done <id>`"),
        "close" => Some("use `tsq done <id>`"),
        "dep" => Some("use `tsq block <task> by <blocker>` or `tsq order <later> after <earlier>`"),
        "link" => Some("use `tsq relate <a> <b>`"),
        "label" if args.get(root_index + 1).map(String::as_str) == Some("add") => {
            Some("use `tsq label <id> <label>`")
        }
        "label" if args.get(root_index + 1).map(String::as_str) == Some("remove") => {
            Some("use `tsq unlabel <id> <label>`")
        }
        "label" if args.get(root_index + 1).map(String::as_str) == Some("list") => {
            Some("use `tsq labels`")
        }
        "note" if args.get(root_index + 1).map(String::as_str) == Some("add") => {
            Some("use `tsq note <id> \"text\"`")
        }
        "note" if args.get(root_index + 1).map(String::as_str) == Some("list") => {
            Some("use `tsq notes <id>`")
        }
        "spec" if args.get(root_index + 1).map(String::as_str) == Some("attach") => {
            Some("use `tsq spec <id> --file spec.md` or `tsq spec <id> --text \"...\"`")
        }
        "spec" if args.get(root_index + 1).map(String::as_str) == Some("check") => {
            Some("use `tsq spec <id> --check`")
        }
        _ => None,
    }
}

fn first_command_token(args: &[String]) -> Option<(usize, &str)> {
    let mut index = 1;
    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "--json" | "--exact-id" => {
                index += 1;
            }
            "--format" => {
                index += 2;
            }
            _ if arg.starts_with("--format=") => {
                index += 1;
            }
            _ if arg.starts_with('-') => {
                index += 1;
            }
            _ => return Some((index, arg)),
        }
    }
    None
}

fn is_missing_subcommand_error(kind: ErrorKind) -> bool {
    kind == ErrorKind::MissingSubcommand
        || kind == ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
}

fn clap_error_exit_code(kind: ErrorKind) -> i32 {
    if kind == ErrorKind::DisplayHelp || kind == ErrorKind::DisplayVersion {
        0
    } else {
        1
    }
}

fn is_init_safe_command(command: &CommandKind) -> bool {
    matches!(
        command,
        CommandKind::Init(_)
            | CommandKind::Doctor
            | CommandKind::MergeDriver(_)
            | CommandKind::Skills { .. }
    )
}

fn root_command_name(command: &CommandKind) -> &'static str {
    match command {
        CommandKind::Init(_) => "init",
        CommandKind::Doctor => "doctor",
        CommandKind::Repair(_) => "repair",
        CommandKind::Orphans => "orphans",
        CommandKind::History(_) => "history",
        CommandKind::Watch(_) => "watch",
        CommandKind::Tui(_) => "tui",
        CommandKind::Create(_) => "create",
        CommandKind::Show(_) => "show",
        CommandKind::Find(_) => "find",
        CommandKind::Stale(_) => "stale",
        CommandKind::Edit(_) => "edit",
        CommandKind::Claim(_) => "claim",
        CommandKind::Assign(_) => "assign",
        CommandKind::Start(_) => "start",
        CommandKind::Open(_) => "open",
        CommandKind::Blocked(_) => "blocked",
        CommandKind::Planned(_) => "planned",
        CommandKind::NeedsPlan(_) => "needs-plan",
        CommandKind::Defer(_) => "defer",
        CommandKind::Done(_) => "done",
        CommandKind::Duplicate(_) => "duplicate",
        CommandKind::Duplicates(_) => "duplicates",
        CommandKind::Supersede(_) => "supersede",
        CommandKind::Merge(_) => "merge",
        CommandKind::Reopen(_) => "reopen",
        CommandKind::Cancel(_) => "cancel",
        CommandKind::Block(_) => "block",
        CommandKind::Unblock(_) => "unblock",
        CommandKind::Order(_) => "order",
        CommandKind::Unorder(_) => "unorder",
        CommandKind::Deps(_) => "deps",
        CommandKind::Relate(_) => "relate",
        CommandKind::Unrelate(_) => "unrelate",
        CommandKind::Label(_) => "label",
        CommandKind::Unlabel(_) => "unlabel",
        CommandKind::Labels => "labels",
        CommandKind::Note(_) => "note",
        CommandKind::Notes(_) => "notes",
        CommandKind::Spec(_) => "spec",
        CommandKind::Sync(_) => "sync",
        CommandKind::Hooks { .. } => "hooks",
        CommandKind::Skills { .. } => "skills",
        CommandKind::Migrate(_) => "migrate",
        CommandKind::MergeDriver(_) => "merge-driver",
    }
}
