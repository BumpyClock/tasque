import { createHash } from "node:crypto";
import { copyFile, mkdir, readdir, rm, stat, writeFile } from "node:fs/promises";
import { basename, extname, join } from "node:path";

interface PackageJson {
  version?: string;
}

const ROOT = process.cwd();
const DIST_DIR = join(ROOT, "dist");
const RELEASE_DIR = join(DIST_DIR, "releases");

async function main(): Promise<void> {
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

  const hash = await sha256Hex(artifactPath);
  await writeFile(join(RELEASE_DIR, "SHA256SUMS.txt"), `${hash}  ${artifactName}\n`, "utf8");

  const size = (await stat(artifactPath)).size;
  console.log(`release artifact: ${artifactPath}`);
  console.log(`size: ${size} bytes`);
  console.log(`sha256: ${hash}`);
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
  const candidates = entries
    .map((name) => join(DIST_DIR, name))
    .filter((filePath) => basename(filePath).startsWith("tsq"));

  for (const candidate of candidates) {
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

await main();
