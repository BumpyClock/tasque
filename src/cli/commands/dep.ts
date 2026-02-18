import type { Command } from "commander";
import type { RunAction, RuntimeDeps } from "../action";
import { parseDepDirection, parseDependencyType, parsePositiveInt } from "../parsers";
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
    .option("--type <type>", "dependency type: blocks|starts_after", "blocks")
    .description("Add dependency")
    .action(async function action(child: string, blocker: string, options: { type?: string }) {
      const depType = parseDependencyType(options.type ?? "blocks");
      await runAction(
        this,
        async (opts) => deps.service.depAdd({ child, blocker, depType, exactId: opts.exactId }),
        {
          jsonData: (data) => data,
          human: (data) =>
            console.log(`added dep ${data.child} -> ${data.blocker} (${data.dep_type})`),
        },
      );
    });

  dep
    .command("remove")
    .argument("<child>", "child task")
    .argument("<blocker>", "blocker task")
    .option("--type <type>", "dependency type: blocks|starts_after", "blocks")
    .description("Remove dependency")
    .action(async function action(child: string, blocker: string, options: { type?: string }) {
      const depType = parseDependencyType(options.type ?? "blocks");
      await runAction(
        this,
        async (opts) => deps.service.depRemove({ child, blocker, depType, exactId: opts.exactId }),
        {
          jsonData: (data) => data,
          human: (data) =>
            console.log(`removed dep ${data.child} -> ${data.blocker} (${data.dep_type})`),
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
