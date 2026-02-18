import type { Command } from "commander";
import { printTaskNote, printTaskNotes } from "../render";
import type { RuntimeDeps, RunAction } from "../action";

export function registerNoteCommands(
  program: Command,
  deps: RuntimeDeps,
  runAction: RunAction,
): void {
  const note = program.command("note").description("Task notes");

  note
    .command("add")
    .argument("<id>", "task id")
    .argument("<text>", "note text")
    .description("Append note to task")
    .action(async function action(id: string, text: string) {
      await runAction(
        this,
        async (opts) => deps.service.noteAdd({ id, text, exactId: opts.exactId }),
        {
          jsonData: (data) => data,
          human: (data) => printTaskNote(data.task_id, data.note),
        },
      );
    });

  note
    .command("list")
    .argument("<id>", "task id")
    .description("List task notes")
    .action(async function action(id: string) {
      await runAction(this, async (opts) => deps.service.noteList({ id, exactId: opts.exactId }), {
        jsonData: (data) => data,
        human: (data) => printTaskNotes(data.task_id, data.notes),
      });
    });
}
