import { afterEach, describe, expect, it } from "bun:test";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { findTasqueRoot } from "../src/app/runtime";

const repos: string[] = [];
let originalCwd: () => string;

async function makeRepo(): Promise<string> {
  const repo = await mkdtemp(join(tmpdir(), "tasque-runtime-root-"));
  repos.push(repo);
  return repo;
}

afterEach(async () => {
  // Restore original process.cwd if it was mocked
  if (originalCwd) {
    process.cwd = originalCwd;
  }
  // Clean up temporary directories
  await Promise.all(repos.splice(0).map((repo) => rm(repo, { recursive: true, force: true })));
});

describe("findTasqueRoot", () => {
  it("finds .tasque/ in current directory", async () => {
    const repo = await makeRepo();
    await mkdir(join(repo, ".tasque"));

    // Save original and mock process.cwd
    originalCwd = process.cwd;
    process.cwd = () => repo;

    const result = findTasqueRoot();
    expect(result).toBe(repo);
  });

  it("finds .tasque/ in parent directory", async () => {
    const repo = await makeRepo();
    await mkdir(join(repo, ".tasque"));
    const subdir = join(repo, "subdir");
    await mkdir(subdir);

    // Save original and mock process.cwd to subdirectory
    originalCwd = process.cwd;
    process.cwd = () => subdir;

    const result = findTasqueRoot();
    expect(result).toBe(repo);
  });

  it("returns null when no .tasque/ exists", async () => {
    const repo = await makeRepo();

    // Save original and mock process.cwd
    originalCwd = process.cwd;
    process.cwd = () => repo;

    const result = findTasqueRoot();
    expect(result).toBeNull();
  });
});
