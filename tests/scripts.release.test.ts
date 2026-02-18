import { afterEach, describe, expect, it } from "bun:test";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { writeChecksumsFile } from "../scripts/release";

const dirs: string[] = [];

async function makeReleaseDir(): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "tasque-release-"));
  dirs.push(dir);
  return dir;
}

afterEach(async () => {
  await Promise.all(dirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});

function parseChecksumLines(content: string): string[] {
  return content
    .trim()
    .split("\n")
    .filter((line) => line.length > 0);
}

describe("release script checksum generation", () => {
  it("includes all files except SHA256SUMS.txt", async () => {
    const releaseDir = await makeReleaseDir();
    await writeFile(join(releaseDir, "tsq-v0.1.0-win-x64.exe"), "binary", "utf8");
    await writeFile(join(releaseDir, "RELEASE_NOTES.md"), "# notes\n", "utf8");
    await writeFile(join(releaseDir, "RELEASE_NOTES.json"), '{"ok":true}\n', "utf8");
    await writeFile(join(releaseDir, "SHA256SUMS.txt"), "stale data\n", "utf8");

    const result = await writeChecksumsFile(releaseDir);

    expect(result.entries.map((entry) => entry.name).sort()).toEqual([
      "RELEASE_NOTES.json",
      "RELEASE_NOTES.md",
      "tsq-v0.1.0-win-x64.exe",
    ]);

    const checksumsContent = await readFile(result.path, "utf8");
    const lines = parseChecksumLines(checksumsContent);
    expect(lines.length).toBe(3);
    expect(lines.some((line) => line.endsWith("SHA256SUMS.txt"))).toBe(false);
  });

  it("writes checksum lines sorted by filename and with matching artifact count", async () => {
    const releaseDir = await makeReleaseDir();
    await writeFile(join(releaseDir, "zeta.bin"), "zeta", "utf8");
    await writeFile(join(releaseDir, "alpha.bin"), "alpha", "utf8");
    await writeFile(join(releaseDir, "middle.txt"), "middle", "utf8");

    const result = await writeChecksumsFile(releaseDir);
    expect(result.entries.map((entry) => entry.name)).toEqual([
      "alpha.bin",
      "middle.txt",
      "zeta.bin",
    ]);

    const checksumsContent = await readFile(result.path, "utf8");
    const lines = parseChecksumLines(checksumsContent);
    expect(lines.length).toBe(3);
    expect(lines.every((line) => /^[a-f0-9]{64} {2}.+/u.test(line))).toBe(true);

    const fileNames = lines.map((line) => line.split("  ")[1]);
    expect(fileNames).toEqual(["alpha.bin", "middle.txt", "zeta.bin"]);
    expect(fileNames.length).toBe(result.entries.length);
  });
});
