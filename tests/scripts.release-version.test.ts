import { afterEach, describe, expect, it } from "bun:test";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { resolveReleaseVersion } from "../scripts/release-version";

const repos: string[] = [];

async function makeRepo(version = "1.2.3"): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-release-version-"));
  repos.push(repo);
  await Bun.write(
    join(repo, "package.json"),
    `${JSON.stringify({ name: "tasque", version, private: true }, null, 2)}\n`,
  );
  return repo;
}

afterEach(async () => {
  await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
});

describe("release version sync", () => {
  it("resolves expected tag from package version", async () => {
    const repo = await makeRepo("2.0.1");
    const info = await resolveReleaseVersion(repo, {});
    expect(info).toEqual({ version: "2.0.1", tag: "v2.0.1" });
  });

  it("accepts matching explicit tag", async () => {
    const repo = await makeRepo("0.9.0");
    const info = await resolveReleaseVersion(repo, { tag: "v0.9.0" });
    expect(info.tag).toBe("v0.9.0");
  });

  it("accepts refs/tags/* tag format", async () => {
    const repo = await makeRepo("0.9.0");
    const info = await resolveReleaseVersion(repo, { tag: "refs/tags/v0.9.0" });
    expect(info.tag).toBe("v0.9.0");
  });

  it("rejects mismatched tag", async () => {
    const repo = await makeRepo("1.0.0");
    await expect(resolveReleaseVersion(repo, { tag: "v1.0.1" })).rejects.toThrow(
      "does not match expected",
    );
  });

  it("rejects mismatched expected version", async () => {
    const repo = await makeRepo("1.0.0");
    await expect(resolveReleaseVersion(repo, { expectedVersion: "1.1.0" })).rejects.toThrow(
      "does not match package.json version",
    );
  });

  it("rejects invalid expected version format", async () => {
    const repo = await makeRepo("1.0.0");
    await expect(resolveReleaseVersion(repo, { expectedVersion: "not-a-semver" })).rejects.toThrow(
      "not valid semver",
    );
  });

  it("rejects invalid semver in package.json", async () => {
    const repo = await makeRepo("not-semver");
    await expect(resolveReleaseVersion(repo, {})).rejects.toThrow(
      "package.json version is missing or invalid semver",
    );
  });

  it("requires package.json to exist", async () => {
    const repo = await mkdtemp(join(tmpdir(), "tasque-release-version-missing-"));
    repos.push(repo);
    await expect(resolveReleaseVersion(repo, {})).rejects.toBeDefined();
  });

  it("persists exact package version string", async () => {
    const repo = await makeRepo("1.2.3-beta.2+build.7");
    const packageText = await readFile(join(repo, "package.json"), "utf8");
    expect(packageText).toContain('"version": "1.2.3-beta.2+build.7"');
    const info = await resolveReleaseVersion(repo, {});
    expect(info.tag).toBe("v1.2.3-beta.2+build.7");
  });
});
