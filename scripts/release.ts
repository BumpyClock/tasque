import { createHash } from "node:crypto";
import { copyFile, mkdir, readdir, rm, stat, writeFile } from "node:fs/promises";
import { basename, extname, join } from "node:path";
import { generateReleaseNotesArtifacts } from "./release-hooks";

interface PackageJson {
  version?: string;
}

const ROOT = process.cwd();
const DIST_DIR = join(ROOT, "dist");
const RELEASE_DIR = join(DIST_DIR, "releases");
const CHECKSUMS_FILE = "SHA256SUMS.txt";

export async function main(): Promise<void> {
  const version = await readVersion();
  await buildBinary();

  const binaryPath = await findCompiledBinary();
  const platform = process.platform;
  const arch = process.arch;
  const ext = extname(binaryPath);
  const artifactName = `tsq-v${version}-${platform}-${arch}${ext}`;
  const artifactPath = join(RELEASE_DIR, artifactName);

  await rm(RELEASE_DIR, { recursive: true, force: true });
  await mkdir(RELEASE_DIR, { recursive: true });
  await copyFile(binaryPath, artifactPath);

  const notesResult = await generateReleaseNotesArtifacts({
    repoRoot: ROOT,
    releaseDir: RELEASE_DIR,
    version,
    tsqBin: binaryPath,
  });
  const checksums = await writeChecksumsFile(RELEASE_DIR);

  const size = (await stat(artifactPath)).size;
  console.log(`release artifact: ${artifactPath}`);
  console.log(`size: ${size} bytes`);
  if (notesResult.baseline) {
    console.log(`baseline tag: ${notesResult.baseline.tag} (${notesResult.baseline.ts})`);
  } else {
    console.log("baseline tag: none");
  }
  console.log(`release notes: ${notesResult.markdownPath}`);
  console.log(`release notes json: ${notesResult.jsonPath}`);
  console.log(`checksums: ${checksums.path} (${checksums.entries.length} files)`);
}

async function readVersion(): Promise<string> {
  const raw = await Bun.file(join(ROOT, "package.json")).text();
  const parsed = JSON.parse(raw) as PackageJson;
  return parsed.version ?? "0.0.0";
}

async function buildBinary(): Promise<void> {
  const proc = Bun.spawn({
    cmd: ["bun", "run", "build"],
    cwd: ROOT,
    stdout: "inherit",
    stderr: "inherit",
  });
  const code = await proc.exited;
  if (code !== 0) {
    throw new Error(`build failed with exit code ${code}`);
  }
}

async function findCompiledBinary(): Promise<string> {
  const entries = await readdir(DIST_DIR);
  const preferred = ["tsq.exe", "tsq"];
  for (const name of preferred) {
    if (!entries.includes(name)) {
      continue;
    }
    const filePath = join(DIST_DIR, name);
    const info = await stat(filePath);
    if (info.isFile()) {
      return filePath;
    }
  }

  const candidates = entries.map((name) => join(DIST_DIR, name));

  for (const candidate of candidates) {
    if (!basename(candidate).startsWith("tsq")) {
      continue;
    }
    const info = await stat(candidate);
    if (info.isFile()) {
      return candidate;
    }
  }

  throw new Error("compiled binary not found in dist/");
}

async function sha256Hex(filePath: string): Promise<string> {
  const bytes = Buffer.from(await Bun.file(filePath).arrayBuffer());
  return createHash("sha256").update(bytes).digest("hex");
}

export async function writeChecksumsFile(
  releaseDir: string,
): Promise<{ path: string; entries: Array<{ name: string; sha256: string }> }> {
  const entries = await readdir(releaseDir);
  const artifactNames: string[] = [];
  for (const name of entries) {
    if (name === CHECKSUMS_FILE) {
      continue;
    }
    const filePath = join(releaseDir, name);
    const info = await stat(filePath);
    if (info.isFile()) {
      artifactNames.push(name);
    }
  }
  artifactNames.sort((a, b) => a.localeCompare(b));

  const checksums: Array<{ name: string; sha256: string }> = [];
  for (const name of artifactNames) {
    const filePath = join(releaseDir, name);
    checksums.push({ name, sha256: await sha256Hex(filePath) });
  }

  const payload = checksums.map((entry) => `${entry.sha256}  ${entry.name}`).join("\n");
  const checksumPath = join(releaseDir, CHECKSUMS_FILE);
  await writeFile(checksumPath, payload.length > 0 ? `${payload}\n` : "", "utf8");
  return { path: checksumPath, entries: checksums };
}

if (import.meta.main) {
  await main();
}
