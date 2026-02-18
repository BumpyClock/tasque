import type { Command } from "commander";
import type { RunAction, RuntimeDeps } from "../action";
import { parseRelationType } from "../parsers";

export function registerLinkCommands(
  program: Command,
  deps: RuntimeDeps,
  runAction: RunAction,
): void {
  const link = program.command("link").description("Task relation links");

  link
    .command("add")
    .argument("<src>", "source task")
    .argument("<dst>", "destination task")
    .requiredOption("--type <type>", "relates_to|replies_to|duplicates|supersedes")
    .description("Add relation")
    .action(async function action(src: string, dst: string, options: { type: string }) {
      await runAction(
        this,
        async (opts) =>
          deps.service.linkAdd({
            src,
            dst,
            type: parseRelationType(options.type),
            exactId: opts.exactId,
          }),
        {
          jsonData: (data) => data,
          human: (data) => console.log(`added link ${data.type}: ${data.src} -> ${data.dst}`),
        },
      );
    });

  link
    .command("remove")
    .argument("<src>", "source task")
    .argument("<dst>", "destination task")
    .requiredOption("--type <type>", "relates_to|replies_to|duplicates|supersedes")
    .description("Remove relation")
    .action(async function action(src: string, dst: string, options: { type: string }) {
      await runAction(
        this,
        async (opts) =>
          deps.service.linkRemove({
            src,
            dst,
            type: parseRelationType(options.type),
            exactId: opts.exactId,
          }),
        {
          jsonData: (data) => data,
          human: (data) => console.log(`removed link ${data.type}: ${data.src} -> ${data.dst}`),
        },
      );
    });
}
