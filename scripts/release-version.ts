import { appendFile, readFile } from "node:fs/promises";
import { join } from "node:path";

interface PackageJsonShape {
  version?: string;
}

interface CliArgs {
  tag?: string;
  expectedVersion?: string;
  help: boolean;
}

interface ReleaseVersionInfo {
  version: string;
  tag: string;
}

const SEMVER_RE =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/u;

function parseArgs(argv: string[]): CliArgs {
  const args: CliArgs = { help: false };
  for (let idx = 0; idx < argv.length; idx += 1) {
    const token = argv[idx];
    switch (token) {
      case "--tag":
        args.tag = readRequiredValue(argv, idx, "--tag");
        idx += 1;
        break;
      case "--expected-version":
        args.expectedVersion = readRequiredValue(argv, idx, "--expected-version");
        idx += 1;
        break;
      case "-h":
      case "--help":
        args.help = true;
        break;
      default:
        throw new Error(`unknown argument: ${token}`);
    }
  }
  return args;
}

function readRequiredValue(argv: string[], index: number, flag: string): string {
  const value = argv[index + 1];
  if (!value || value.startsWith("-")) {
    throw new Error(`missing value for ${flag}`);
  }
  return value;
}

function normalizeTag(rawTag: string): string {
  if (rawTag.startsWith("refs/tags/")) {
    return rawTag.slice("refs/tags/".length);
  }
  return rawTag;
}

async function readPackageVersion(repoRoot: string): Promise<string> {
  const raw = await readFile(join(repoRoot, "package.json"), "utf8");
  const parsed = JSON.parse(raw) as PackageJsonShape;
  if (!parsed.version || !SEMVER_RE.test(parsed.version)) {
    throw new Error("package.json version is missing or invalid semver");
  }
  return parsed.version;
}

export async function resolveReleaseVersion(
  repoRoot: string,
  options: { tag?: string; expectedVersion?: string },
): Promise<ReleaseVersionInfo> {
  const version = await readPackageVersion(repoRoot);
  const expectedTag = `v${version}`;

  if (options.expectedVersion && !SEMVER_RE.test(options.expectedVersion)) {
    throw new Error(`provided expected version is not valid semver: ${options.expectedVersion}`);
  }

  if (options.expectedVersion && options.expectedVersion !== version) {
    throw new Error(
      `provided version ${options.expectedVersion} does not match package.json version ${version}`,
    );
  }

  if (options.tag) {
    const normalized = normalizeTag(options.tag);
    if (normalized !== expectedTag) {
      throw new Error(`release tag ${normalized} does not match expected ${expectedTag}`);
    }
  }

  return { version, tag: expectedTag };
}

function usage(): string {
  return [
    "Usage: bun run scripts/release-version.ts [options]",
    "",
    "Options:",
    "  --tag <tag>                    Validate release tag matches package.json version",
    "  --expected-version <semver>    Validate provided version matches package.json",
    "  -h, --help                     Show help",
  ].join("\n");
}

async function writeGithubOutputs(info: ReleaseVersionInfo): Promise<void> {
  const outputPath = process.env.GITHUB_OUTPUT;
  if (!outputPath) {
    return;
  }
  const payload = `version=${info.version}\ntag=${info.tag}\n`;
  await appendFile(outputPath, payload, "utf8");
}

export async function main(argv: string[] = Bun.argv.slice(2)): Promise<void> {
  const args = parseArgs(argv);
  if (args.help) {
    console.log(usage());
    return;
  }

  const info = await resolveReleaseVersion(process.cwd(), {
    tag: args.tag,
    expectedVersion: args.expectedVersion,
  });
  await writeGithubOutputs(info);

  console.log(`package version: ${info.version}`);
  console.log(`release tag: ${info.tag}`);
}

if (import.meta.main) {
  try {
    await main();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error(`release-version check failed: ${message}`);
    process.exitCode = 1;
  }
}
