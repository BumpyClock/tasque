import { Command } from "commander";
import packageJson from "../../package.json";
import { TsqError } from "../errors";
import { errEnvelope, okEnvelope } from "../output";
import type { ActionRender, RuntimeDeps } from "./action";
import { registerDepCommands } from "./commands/dep";
import { registerLabelCommands } from "./commands/label";
import { registerLinkCommands } from "./commands/link";
import { registerMetaCommands } from "./commands/meta";
import { registerNoteCommands } from "./commands/note";
import { registerSpecCommands } from "./commands/spec";
import { registerTaskCommands } from "./commands/task";
import type { GlobalOpts } from "./parsers";

const INIT_SAFE_COMMANDS = new Set(["init", "doctor"]);

export function buildProgram(deps: RuntimeDeps): Command {
  const program = new Command();
  program
    .name("tsq")
    .description("Local durable task graph for coding agents")
    .version(packageJson.version, "-V, --version", "output the current version")
    .option("--json", "emit JSON envelope")
    .option("--exact-id", "require exact task ID match");

  program.hook("preAction", (_thisCommand, actionCommand) => {
    // Reset any previously-set exit code so each command invocation starts clean.
    process.exitCode = 0;
    const rootCmd = resolveRootCommandName(actionCommand);
    if (!INIT_SAFE_COMMANDS.has(rootCmd) && deps.findTasqueRoot() === null) {
      throw new TsqError("NOT_INITIALIZED", "No .tasque directory found. Run 'tsq init' first.", 2);
    }
  });

  registerMetaCommands(program, deps, runAction);
  registerTaskCommands(program, deps, runAction);
  registerNoteCommands(program, deps, runAction);
  registerSpecCommands(program, deps, runAction);
  registerDepCommands(program, deps, runAction);
  registerLinkCommands(program, deps, runAction);
  registerLabelCommands(program, deps, runAction);

  return program;
}

async function runAction<TValue, TJson>(
  command: Command,
  action: (opts: GlobalOpts) => Promise<TValue>,
  render: ActionRender<TValue, TJson>,
): Promise<void> {
  const commandLine = commandPath(command);
  const options = command.optsWithGlobals<GlobalOpts>();
  try {
    const value = await action(options);
    if (options.json) {
      console.log(JSON.stringify(okEnvelope(commandLine, render.jsonData(value)), null, 2));
      return;
    }
    render.human(value);
  } catch (error) {
    const tsqError = asTsqError(error);
    if (options.json) {
      console.log(
        JSON.stringify(
          errEnvelope(commandLine, tsqError.code, tsqError.message, tsqError.details),
          null,
          2,
        ),
      );
    } else {
      console.error(`${tsqError.code}: ${tsqError.message}`);
      if (tsqError.details) {
        console.error(JSON.stringify(tsqError.details));
      }
    }
    process.exitCode = tsqError.exitCode;
  }
}

function commandPath(command: Command): string {
  const names: string[] = [];
  let cursor: Command | null = command;
  while (cursor) {
    names.push(cursor.name());
    cursor = cursor.parent ?? null;
  }
  return names.reverse().join(" ");
}

function asTsqError(error: unknown): TsqError {
  if (error instanceof TsqError) {
    return error;
  }
  const message = error instanceof Error ? error.message : "unexpected error";
  return new TsqError("INTERNAL_ERROR", message, 2);
}

function resolveRootCommandName(command: Command): string {
  let cursor: Command = command;
  while (cursor.parent?.parent) {
    cursor = cursor.parent;
  }
  return cursor.name();
}
