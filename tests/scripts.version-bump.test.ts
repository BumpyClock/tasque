import { afterEach, describe, expect, it } from "bun:test";
import { mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runVersionBump } from "../scripts/version-bump";

const repos: string[] = [];

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-version-bump-"));
  repos.push(repo);
  return repo;
}

async function seedRepo(repo: string): Promise<void> {
  await mkdir(join(repo, "src"), { recursive: true });
  await mkdir(join(repo, "SKILLS", "tasque", "references"), { recursive: true });
  await Bun.write(
    join(repo, "package.json"),
    JSON.stringify({ name: "tasque", version: "1.2.3", private: true }, null, 2).concat("\n"),
  );
  await Bun.write(join(repo, "src", "types.ts"), "export const SCHEMA_VERSION = 1;\n");
  await Bun.write(
    join(repo, "README.md"),
    ["{", '  "schema_version": 1,', '  "ok": true', "}"].join("\n").concat("\n"),
  );
  await Bun.write(
    join(repo, "SKILLS", "tasque", "references", "machine-output-and-durability.md"),
    ["```json", "{", '  "schema_version": 1', "}", "```"].join("\n").concat("\n"),
  );
}

afterEach(async () => {
  await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
});

describe("version bump script", () => {
  it("bumps package version using --bump patch", async () => {
    const repo = await makeRepo();
    await seedRepo(repo);

    const result = await runVersionBump(repo, { bump: "patch", dryRun: false, help: false });
    expect(result.version).toEqual({ from: "1.2.3", to: "1.2.4" });

    const pkg = JSON.parse(await readFile(join(repo, "package.json"), "utf8")) as {
      version: string;
    };
    expect(pkg.version).toBe("1.2.4");
  });

  it("supports explicit version in dry-run mode without writing", async () => {
    const repo = await makeRepo();
    await seedRepo(repo);

    const result = await runVersionBump(repo, {
      version: "2.0.0",
      dryRun: true,
      help: false,
    });
    expect(result.version).toEqual({ from: "1.2.3", to: "2.0.0" });

    const pkg = JSON.parse(await readFile(join(repo, "package.json"), "utf8")) as {
      version: string;
    };
    expect(pkg.version).toBe("1.2.3");
  });

  it("bumps schema version in code and docs", async () => {
    const repo = await makeRepo();
    await seedRepo(repo);

    const result = await runVersionBump(repo, { schema: 2, dryRun: false, help: false });
    expect(result.schema).toEqual({ from: 1, to: 2 });

    const typesText = await readFile(join(repo, "src", "types.ts"), "utf8");
    expect(typesText).toContain("export const SCHEMA_VERSION = 2;");

    const readme = await readFile(join(repo, "README.md"), "utf8");
    expect(readme).toContain('"schema_version": 2');

    const skillRef = await readFile(
      join(repo, "SKILLS", "tasque", "references", "machine-output-and-durability.md"),
      "utf8",
    );
    expect(skillRef).toContain('"schema_version": 2');
  });

  it("rejects empty invocation", async () => {
    const repo = await makeRepo();
    await seedRepo(repo);
    await expect(runVersionBump(repo, { dryRun: false, help: false })).rejects.toThrow(
      "provide at least one of --version, --bump, or --schema",
    );
  });
});
