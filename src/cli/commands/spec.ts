import type { Command } from "commander";
import type { RunAction, RuntimeDeps } from "../action";
import { type SpecAttachCommandOptions, asOptionalString } from "../parsers";
import { printTask } from "../render";

export function registerSpecCommands(
  program: Command,
  deps: RuntimeDeps,
  runAction: RunAction,
): void {
  const spec = program.command("spec").description("Task specification operations");

  spec
    .command("attach")
    .argument("<id>", "task id")
    .argument("[source]", "markdown source file path (shorthand for --file)")
    .option("--file <path>", "markdown source file path")
    .option("--stdin", "read markdown source from stdin")
    .option("--text <markdown>", "inline markdown source")
    .option("--force", "overwrite existing spec with different fingerprint")
    .description("Attach markdown spec to a task")
    .action(async function action(
      id: string,
      source: string | undefined,
      options: SpecAttachCommandOptions,
    ) {
      await runAction(
        this,
        async (opts) =>
          deps.service.specAttach({
            id,
            source: asOptionalString(source),
            file: asOptionalString(options.file),
            stdin: Boolean(options.stdin),
            text: typeof options.text === "string" ? options.text : undefined,
            force: Boolean(options.force),
            exactId: opts.exactId,
          }),
        {
          jsonData: (data) => data,
          human: (data) => {
            printTask(data.task);
            console.log(`spec=${data.spec.spec_path}`);
            console.log(`spec_sha256=${data.spec.spec_fingerprint}`);
          },
        },
      );
    });

  spec
    .command("check")
    .argument("<id>", "task id")
    .description("Validate attached markdown spec")
    .action(async function action(id: string) {
      await runAction(this, async (opts) => deps.service.specCheck({ id, exactId: opts.exactId }), {
        jsonData: (data) => data,
        human: (data) => {
          console.log(`task=${data.task_id}`);
          console.log(`spec_ok=${data.ok}`);
          if (data.spec.spec_path) {
            console.log(`spec=${data.spec.spec_path}`);
          }
          if (data.spec.expected_fingerprint) {
            console.log(`spec_sha256_expected=${data.spec.expected_fingerprint}`);
          }
          if (data.spec.actual_fingerprint) {
            console.log(`spec_sha256_actual=${data.spec.actual_fingerprint}`);
          }
          if (data.spec.missing_sections.length > 0) {
            console.log(`missing_sections=${data.spec.missing_sections.join(",")}`);
          }
          for (const diagnostic of data.diagnostics) {
            console.log(`diagnostic=${diagnostic.code}:${diagnostic.message}`);
          }
        },
      });
    });
}
