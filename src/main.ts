#!/usr/bin/env bun
import { findTasqueRoot, getActor, getRepoRoot, nowIso } from "./app/runtime";
import { TasqueService } from "./app/service";
import { buildProgram } from "./cli/program";
import { TsqError } from "./errors";
import { errEnvelope } from "./output";

async function main(): Promise<void> {
  const repoRoot = getRepoRoot();
  const actor = getActor(repoRoot);
  const service = new TasqueService(repoRoot, actor, nowIso);
  const program = buildProgram({ service, findTasqueRoot });

  try {
    await program.parseAsync(process.argv);
  } catch (error) {
    const isJsonMode = process.argv.includes("--json");
    const tsqError =
      error instanceof TsqError
        ? error
        : new TsqError(
            "INTERNAL_ERROR",
            error instanceof Error ? error.message : "unexpected error",
            2,
          );

    if (isJsonMode) {
      const commandName = process.argv.slice(2).find((arg) => !arg.startsWith("-")) || "tsq";
      console.log(
        JSON.stringify(
          errEnvelope(`tsq ${commandName}`, tsqError.code, tsqError.message, tsqError.details),
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

await main();
