import type { Command } from "commander";
import { printLabelList, printTask } from "../render";
import type { RuntimeDeps, RunAction } from "../action";

export function registerLabelCommands(
  program: Command,
  deps: RuntimeDeps,
  runAction: RunAction,
): void {
  const label = program.command("label").description("Label operations");

  label
    .command("add")
    .argument("<id>", "task id")
    .argument("<label>", "label to add")
    .description("Add label to task")
    .action(async function action(id: string, labelStr: string) {
      await runAction(
        this,
        async (opts) => deps.service.labelAdd({ id, label: labelStr, exactId: opts.exactId }),
        {
          jsonData: (task) => ({ task }),
          human: (task) => printTask(task),
        },
      );
    });

  label
    .command("remove")
    .argument("<id>", "task id")
    .argument("<label>", "label to remove")
    .description("Remove label from task")
    .action(async function action(id: string, labelStr: string) {
      await runAction(
        this,
        async (opts) => deps.service.labelRemove({ id, label: labelStr, exactId: opts.exactId }),
        {
          jsonData: (task) => ({ task }),
          human: (task) => printTask(task),
        },
      );
    });

  label
    .command("list")
    .description("List all labels with counts")
    .action(async function action() {
      await runAction(this, async () => deps.service.labelList(), {
        jsonData: (data) => ({ labels: data }),
        human: (data) => printLabelList(data),
      });
    });
}
