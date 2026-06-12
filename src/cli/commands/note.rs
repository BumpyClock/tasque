use crate::app::service::TasqueService;
use crate::app::service_types::{NoteAddInput, NoteListInput};
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::render::{print_task_note, print_task_notes};
use crate::errors::TsqError;
use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum NoteCommand {
    Add(NoteAddArgs),
    List(NoteListArgs),
}

#[derive(Debug, Args)]
pub struct NoteAddArgs {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Args)]
pub struct NoteListArgs {
    pub id: String,
}

#[derive(Debug, Args)]
#[command(after_help = "Examples:
  tsq note tsq-abc12345 \"blocked on API decision\"
  printf 'multi-line note' | tsq note tsq-abc12345 --stdin
  tsq notes tsq-abc12345")]
pub struct NoteArgs {
    pub id: String,
    pub text: Option<String>,
    #[arg(long)]
    pub stdin: bool,
}

pub fn execute_note(service: &TasqueService, command: NoteCommand, opts: GlobalOpts) -> i32 {
    match command {
        NoteCommand::Add(args) => {
            run_note_add(service, "tsq note add", opts, args.id, Ok(args.text))
        }
        NoteCommand::List(args) => run_note_list(service, "tsq note list", opts, args.id),
    }
}

pub fn execute_note_verb(service: &TasqueService, args: NoteArgs, opts: GlobalOpts) -> i32 {
    run_note_add(service, "tsq note", opts, args.id.clone(), note_text(&args))
}

pub fn execute_notes_verb(service: &TasqueService, args: NoteListArgs, opts: GlobalOpts) -> i32 {
    run_note_list(service, "tsq notes", opts, args.id)
}

fn run_note_add(
    service: &TasqueService,
    command_line: &'static str,
    opts: GlobalOpts,
    id: String,
    text: Result<String, TsqError>,
) -> i32 {
    run_action(
        command_line,
        opts,
        || {
            service.note_add(NoteAddInput {
                id,
                text: text?,
                exact_id: opts.exact_id,
            })
        },
        |data| data.clone(),
        |data| {
            print_task_note(&data.task_id, &data.note);
            Ok(())
        },
    )
}

fn run_note_list(
    service: &TasqueService,
    command_line: &'static str,
    opts: GlobalOpts,
    id: String,
) -> i32 {
    run_action(
        command_line,
        opts,
        || {
            service.note_list(NoteListInput {
                id,
                exact_id: opts.exact_id,
            })
        },
        |data| data.clone(),
        |data| {
            print_task_notes(&data.task_id, &data.notes);
            Ok(())
        },
    )
}

fn note_text(args: &NoteArgs) -> Result<String, TsqError> {
    match (&args.text, args.stdin) {
        (Some(_), true) => Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine note text with --stdin",
            1,
        )),
        (Some(text), false) => Ok(text.clone()),
        (None, true) => crate::app::stdin::read_stdin_content(),
        (None, false) => Err(TsqError::new(
            "VALIDATION_ERROR",
            "note text is required unless --stdin is provided",
            1,
        )),
    }
}
