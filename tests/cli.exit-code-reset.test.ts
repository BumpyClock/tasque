import { afterEach, describe, expect, it } from "bun:test";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { nowIso } from "../src/app/runtime";
import { TasqueService } from "../src/app/service";
import { buildProgram } from "../src/cli/program";
import { cleanupRepos, makeRepo as makeRepoBase } from "./helpers";

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-exitcode-");
}

afterEach(async () => {
  process.exitCode = 0;
  await cleanupRepos();
});

describe("CLI exit code reset", () => {
  it("clears a previous error exit code before the next successful command", async () => {
    const repo = await makeRepo();
    const service = new TasqueService(repo, "exitcode-test", nowIso);
    const findTasqueRoot = () => (existsSync(join(repo, ".tasque")) ? repo : null);
    const program = buildProgram({ service, findTasqueRoot });

    const originalLog = console.log;
    const originalError = console.error;
    console.log = () => {};
    console.error = () => {};

    try {
      await program.parseAsync(["init"], { from: "user" });

      process.exitCode = 0;
      await program.parseAsync(["show", "missing-task-id"], { from: "user" });
      expect(process.exitCode).toBe(1);

      await program.parseAsync(["list"], { from: "user" });
      expect(process.exitCode).toBe(0);
    } finally {
      console.log = originalLog;
      console.error = originalError;
    }
  });
});
