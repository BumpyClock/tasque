#!/usr/bin/env bun
import { getActor, getRepoRoot, nowIso } from "./app/runtime";
import { TasqueService } from "./app/service";
import { buildProgram } from "./cli/program";

async function main(): Promise<void> {
  const repoRoot = getRepoRoot();
  const actor = getActor(repoRoot);
  const service = new TasqueService(repoRoot, actor, nowIso);
  const program = buildProgram({ service });
  await program.parseAsync(process.argv);
}

await main();
