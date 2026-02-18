import { afterEach, describe, expect, it } from "bun:test";
import packageJson from "../package.json";
import { cleanupRepos, makeRepo, runCli } from "./helpers";

async function makeTestRepo(): Promise<string> {
  return makeRepo("tasque-version-");
}

afterEach(cleanupRepos);

describe("tsq version flag", () => {
  it("outputs version with -V flag", async () => {
    const repoDir = await makeTestRepo();
    const result = await runCli(repoDir, ["-V"]);
    expect(result.exitCode).toBe(0);
    expect(result.stdout.trim()).toBe(packageJson.version);
    expect(result.stderr).toBe("");
  });

  it("outputs version with --version flag", async () => {
    const repoDir = await makeTestRepo();
    const result = await runCli(repoDir, ["--version"]);
    expect(result.exitCode).toBe(0);
    expect(result.stdout.trim()).toBe(packageJson.version);
    expect(result.stderr).toBe("");
  });
});
