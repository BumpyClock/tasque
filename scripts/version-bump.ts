import { readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";

type BumpType = "major" | "minor" | "patch";

const SEMVER_RE =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/u;

const SCHEMA_DOC_FILES = [
  "README.md",
  "SKILLS/tasque/references/machine-output-and-durability.md",
] as const;

interface PackageJsonShape {
  version?: string;
  [key: string]: unknown;
}

interface CliArgs {
  version?: string;
  bump?: BumpType;
  schema?: number;
  dryRun: boolean;
  help: boolean;
}

interface FileChange {
  path: string;
  reason: string;
}

export interface VersionBumpResult {
  repoRoot: string;
  dryRun: boolean;
  version?: { from: string; to: string };
  schema?: { from: number; to: number };
  changes: FileChange[];
}

export async function runVersionBump(repoRoot: string, args: CliArgs): Promise<VersionBumpResult> {
  validateArgs(args);

  const absoluteRoot = resolve(repoRoot);
  const result: VersionBumpResult = {
    repoRoot: absoluteRoot,
    dryRun: args.dryRun,
    changes: [],
  };

  if (args.version || args.bump) {
    const packagePath = join(absoluteRoot, "package.json");
    const packageRaw = await readFile(packagePath, "utf8");
    const pkg = JSON.parse(packageRaw) as PackageJsonShape;
    const currentVersion = pkg.version;
    if (typeof currentVersion !== "string" || !SEMVER_RE.test(currentVersion)) {
      throw new Error("package.json version is missing or invalid semver");
    }

    const targetVersion = args.version ?? bumpSemver(currentVersion, args.bump as BumpType);
    if (!SEMVER_RE.test(targetVersion)) {
      throw new Error(`invalid target semver: ${targetVersion}`);
    }

    if (targetVersion !== currentVersion) {
      pkg.version = targetVersion;
      if (!args.dryRun) {
        await writeFile(packagePath, `${JSON.stringify(pkg, null, 2)}\n`, "utf8");
      }
      result.version = { from: currentVersion, to: targetVersion };
      result.changes.push({ path: "package.json", reason: "package version" });
    }
  }

  if (typeof args.schema === "number") {
    const typesPath = join(absoluteRoot, "src", "types.ts");
    const typesRaw = await readFile(typesPath, "utf8");
    const match = typesRaw.match(/export const SCHEMA_VERSION = (\d+);/u);
    if (!match) {
      throw new Error("src/types.ts missing SCHEMA_VERSION constant");
    }

    const currentSchema = Number.parseInt(match[1] ?? "", 10);
    if (currentSchema !== args.schema) {
      const nextTypes = typesRaw.replace(
        /export const SCHEMA_VERSION = \d+;/u,
        `export const SCHEMA_VERSION = ${args.schema};`,
      );
      if (!args.dryRun) {
        await writeFile(typesPath, nextTypes, "utf8");
      }
      result.schema = { from: currentSchema, to: args.schema };
      result.changes.push({ path: "src/types.ts", reason: "schema version constant" });
    }

    for (const relativePath of SCHEMA_DOC_FILES) {
      const absolutePath = join(absoluteRoot, relativePath);
      let docRaw: string;
      try {
        docRaw = await readFile(absolutePath, "utf8");
      } catch (error) {
        const code = (error as NodeJS.ErrnoException).code;
        if (code === "ENOENT") {
          continue;
        }
        throw error;
      }

      const replaced = docRaw.replace(/("schema_version"\s*:\s*)\d+/gu, `$1${String(args.schema)}`);
      if (replaced === docRaw) {
        continue;
      }
      if (!args.dryRun) {
        await writeFile(absolutePath, replaced, "utf8");
      }
      result.changes.push({ path: relativePath, reason: "schema version examples" });
    }
  }

  return result;
}

function parseArgs(argv: string[]): CliArgs {
  const args: CliArgs = { dryRun: false, help: false };

  for (let idx = 0; idx < argv.length; idx += 1) {
    const token = argv[idx];
    switch (token) {
      case "--version": {
        args.version = readRequiredValue(argv, idx, "--version");
        idx += 1;
        break;
      }
      case "--bump": {
        const value = readRequiredValue(argv, idx, "--bump");
        if (value !== "major" && value !== "minor" && value !== "patch") {
          throw new Error('--bump must be one of "major", "minor", "patch"');
        }
        args.bump = value;
        idx += 1;
        break;
      }
      case "--schema": {
        const value = readRequiredValue(argv, idx, "--schema");
        const schema = Number.parseInt(value, 10);
        if (!Number.isInteger(schema) || schema <= 0) {
          throw new Error("--schema must be a positive integer");
        }
        args.schema = schema;
        idx += 1;
        break;
      }
      case "--dry-run":
        args.dryRun = true;
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

function validateArgs(args: CliArgs): void {
  if (args.help) {
    return;
  }
  if (args.version && args.bump) {
    throw new Error("use either --version or --bump, not both");
  }
  if (args.version && !SEMVER_RE.test(args.version)) {
    throw new Error("--version must be a valid semver (e.g. 1.2.3)");
  }
  if (!args.version && !args.bump && typeof args.schema !== "number") {
    throw new Error("provide at least one of --version, --bump, or --schema");
  }
}

function bumpSemver(version: string, bump: BumpType): string {
  const match = version.match(SEMVER_RE);
  if (!match) {
    throw new Error(`cannot bump invalid semver: ${version}`);
  }
  const major = Number.parseInt(match[1] ?? "0", 10);
  const minor = Number.parseInt(match[2] ?? "0", 10);
  const patch = Number.parseInt(match[3] ?? "0", 10);
  if (bump === "major") {
    return `${major + 1}.0.0`;
  }
  if (bump === "minor") {
    return `${major}.${minor + 1}.0`;
  }
  return `${major}.${minor}.${patch + 1}`;
}

function usage(): string {
  return [
    "Usage: bun run scripts/version-bump.ts [options]",
    "",
    "Options:",
    "  --version <semver>         Set package.json version directly",
    "  --bump <major|minor|patch> Increment package.json version",
    "  --schema <number>          Set src/types.ts SCHEMA_VERSION and docs examples",
    "  --dry-run                  Print planned changes without writing",
    "  -h, --help                 Show help",
  ].join("\n");
}

function printSummary(result: VersionBumpResult): void {
  if (result.changes.length === 0) {
    console.log("No version changes needed.");
    return;
  }
  const mode = result.dryRun ? "dry-run planned changes:" : "updated files:";
  console.log(mode);
  for (const change of result.changes) {
    console.log(`- ${change.path} (${change.reason})`);
  }
  if (result.version) {
    console.log(`package version: ${result.version.from} -> ${result.version.to}`);
  }
  if (result.schema) {
    console.log(`schema version: ${result.schema.from} -> ${result.schema.to}`);
  }
}

export async function main(argv: string[] = Bun.argv.slice(2)): Promise<void> {
  const args = parseArgs(argv);
  if (args.help) {
    console.log(usage());
    return;
  }
  const result = await runVersionBump(process.cwd(), args);
  printSummary(result);
}

if (import.meta.main) {
  try {
    await main();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error(`version-bump failed: ${message}`);
    process.exitCode = 1;
  }
}
