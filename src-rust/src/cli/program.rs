use crate::app::runtime::find_tasque_root;
use crate::app::service::TasqueService;
use crate::cli::action::GlobalOpts;
use crate::cli::commands::{
    DepCommand, LabelCommand, LinkCommand, NoteCommand, SpecCommand, execute_dep, execute_label,
    execute_link, execute_note, execute_spec,
};
use crate::cli::commands::{meta, task};
use crate::output::err_envelope;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "tsq")]
#[command(version)]
#[command(about = "Local durable task graph for coding agents")]
pub struct Cli {
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long = "exact-id", global = true)]
    pub exact_id: bool,
    #[command(subcommand)]
    pub command: CommandKind,
}

#[derive(Debug, Subcommand)]
pub enum CommandKind {
    Init(meta::InitArgs),
    Doctor,
    Repair(meta::RepairArgs),
    Orphans,
    History(meta::HistoryArgs),
    Watch(meta::WatchArgs),
    Create(task::CreateArgs),
    Show(task::ShowArgs),
    List(task::ListArgs),
    Stale(task::StaleArgs),
    Ready(task::ReadyArgs),
    Update(task::UpdateArgs),
    Duplicate(task::DuplicateArgs),
    Duplicates(task::DuplicatesArgs),
    Supersede(task::SupersedeArgs),
    Merge(task::MergeArgs),
    Close(task::CloseArgs),
    Reopen(task::ReopenArgs),
    Search(task::SearchArgs),
    Dep {
        #[command(subcommand)]
        command: DepCommand,
    },
    Link {
        #[command(subcommand)]
        command: LinkCommand,
    },
    Label {
        #[command(subcommand)]
        command: LabelCommand,
    },
    Note {
        #[command(subcommand)]
        command: NoteCommand,
    },
    Spec {
        #[command(subcommand)]
        command: SpecCommand,
    },
}

pub fn run_cli(service: &TasqueService) -> i32 {
    let cli = Cli::parse();
    let opts = GlobalOpts {
        json: cli.json,
        exact_id: cli.exact_id,
    };

    if !is_init_safe_command(&cli.command) && find_tasque_root().is_none() {
        let code = "NOT_INITIALIZED";
        let message = "No .tasque directory found. Run 'tsq init' first.";
        if opts.json {
            let command_line = format!("tsq {}", root_command_name(&cli.command));
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

    match cli.command {
        CommandKind::Init(args) => meta::execute_init(service, args, opts),
        CommandKind::Doctor => meta::execute_doctor(service, opts),
        CommandKind::Repair(args) => meta::execute_repair(service, args, opts),
        CommandKind::Orphans => meta::execute_orphans(service, opts),
        CommandKind::History(args) => meta::execute_history(service, args, opts),
        CommandKind::Watch(args) => meta::execute_watch(service, args, opts),
        CommandKind::Create(args) => task::execute_create(service, args, opts),
        CommandKind::Show(args) => task::execute_show(service, args, opts),
        CommandKind::List(args) => task::execute_list(service, args, opts),
        CommandKind::Stale(args) => task::execute_stale(service, args, opts),
        CommandKind::Ready(args) => task::execute_ready(service, args, opts),
        CommandKind::Update(args) => task::execute_update(service, args, opts),
        CommandKind::Duplicate(args) => task::execute_duplicate(service, args, opts),
        CommandKind::Duplicates(args) => task::execute_duplicates(service, args, opts),
        CommandKind::Supersede(args) => task::execute_supersede(service, args, opts),
        CommandKind::Merge(args) => task::execute_merge(service, args, opts),
        CommandKind::Close(args) => task::execute_close(service, args, opts),
        CommandKind::Reopen(args) => task::execute_reopen(service, args, opts),
        CommandKind::Search(args) => task::execute_search(service, args, opts),
        CommandKind::Dep { command } => execute_dep(service, command, opts),
        CommandKind::Link { command } => execute_link(service, command, opts),
        CommandKind::Label { command } => execute_label(service, command, opts),
        CommandKind::Note { command } => execute_note(service, command, opts),
        CommandKind::Spec { command } => execute_spec(service, command, opts),
    }
}

fn is_init_safe_command(command: &CommandKind) -> bool {
    matches!(command, CommandKind::Init(_) | CommandKind::Doctor)
}

fn root_command_name(command: &CommandKind) -> &'static str {
    match command {
        CommandKind::Init(_) => "init",
        CommandKind::Doctor => "doctor",
        CommandKind::Repair(_) => "repair",
        CommandKind::Orphans => "orphans",
        CommandKind::History(_) => "history",
        CommandKind::Watch(_) => "watch",
        CommandKind::Create(_) => "create",
        CommandKind::Show(_) => "show",
        CommandKind::List(_) => "list",
        CommandKind::Stale(_) => "stale",
        CommandKind::Ready(_) => "ready",
        CommandKind::Update(_) => "update",
        CommandKind::Duplicate(_) => "duplicate",
        CommandKind::Duplicates(_) => "duplicates",
        CommandKind::Supersede(_) => "supersede",
        CommandKind::Merge(_) => "merge",
        CommandKind::Close(_) => "close",
        CommandKind::Reopen(_) => "reopen",
        CommandKind::Search(_) => "search",
        CommandKind::Dep { .. } => "dep",
        CommandKind::Link { .. } => "link",
        CommandKind::Label { .. } => "label",
        CommandKind::Note { .. } => "note",
        CommandKind::Spec { .. } => "spec",
    }
}
