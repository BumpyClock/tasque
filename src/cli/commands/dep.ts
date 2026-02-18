import type { Command } from "commander";
import type { RunAction, RuntimeDeps } from "../action";
import { parseDepDirection, parsePositiveInt } from "../parsers";
import { printDepTreeResult } from "../render";

export function registerDepCommands(
  program: Command,
  deps: RuntimeDeps,
  runAction: RunAction,
): void {
  const dep = program.command("dep").description("Dependency operations");

  dep
    .command("add")
    .argument("<child>", "child task")
    .argument("<blocker>", "blocker task")
    .description("Add dependency")
    .action(async function action(child: string, blocker: string) {
      await runAction(
        this,
        async (opts) => deps.service.depAdd({ child, blocker, exactId: opts.exactId }),
        {
          jsonData: (data) => data,
          human: (data) => console.log(`added dep ${data.child} -> ${data.blocker}`),
        },
      );
    });

  dep
    .command("remove")
    .argument("<child>", "child task")
    .argument("<blocker>", "blocker task")
    .description("Remove dependency")
    .action(async function action(child: string, blocker: string) {
      await runAction(
        this,
        async (opts) => deps.service.depRemove({ child, blocker, exactId: opts.exactId }),
        {
          jsonData: (data) => data,
          human: (data) => console.log(`removed dep ${data.child} -> ${data.blocker}`),
        },
      );
    });

  dep
    .command("tree")
    .argument("<id>", "root task id")
    .option("--direction <dir>", "up|down|both", "both")
    .option("--depth <n>", "max depth")
    .description("Show dependency tree")
    .action(async function action(id: string, options: { direction?: string; depth?: string }) {
      await runAction(
        this,
        async (opts) =>
          deps.service.depTree({
            id,
            direction: parseDepDirection(options.direction),
            depth: options.depth ? parsePositiveInt(options.depth, "depth", 1, 100) : undefined,
            exactId: opts.exactId,
          }),
        {
          jsonData: (root) => ({ root }),
          human: (root) => printDepTreeResult(root),
        },
      );
    });
}
