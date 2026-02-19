use crate::app::service::TasqueService;
use crate::app::service_types::{NoteAddInput, NoteListInput};
use crate::cli::action::{GlobalOpts, run_action};
use crate::cli::render::{print_task_note, print_task_notes};
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

pub fn execute_note(service: &TasqueService, command: NoteCommand, opts: GlobalOpts) -> i32 {
    match command {
        NoteCommand::Add(args) => run_action(
            "tsq note add",
            opts,
            || {
                service.note_add(NoteAddInput {
                    id: args.id.clone(),
                    text: args.text.clone(),
                    exact_id: opts.exact_id,
                })
            },
            |data| data.clone(),
            |data| {
                print_task_note(&data.task_id, &data.note);
                Ok(())
            },
        ),
        NoteCommand::List(args) => run_action(
            "tsq note list",
            opts,
            || {
                service.note_list(NoteListInput {
                    id: args.id.clone(),
                    exact_id: opts.exact_id,
                })
            },
            |data| data.clone(),
            |data| {
                print_task_notes(&data.task_id, &data.notes);
                Ok(())
            },
        ),
    }
}
