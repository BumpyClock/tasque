#!/usr/bin/env node
"use strict";

const fs = require("fs");
const path = require("path");

const CARGO_TOML = path.join(__dirname, "..", "..", "Cargo.toml");
const ROOT_PKG = path.join(__dirname, "..", "package.json");
const PLATFORM_PKGS = [
  path.join(__dirname, "..", "platforms", "darwin-arm64", "package.json"),
  path.join(__dirname, "..", "platforms", "darwin-x64", "package.json"),
  path.join(__dirname, "..", "platforms", "linux-x64-gnu", "package.json"),
  path.join(__dirname, "..", "platforms", "linux-arm64-gnu", "package.json"),
  path.join(__dirname, "..", "platforms", "win32-x64-msvc", "package.json"),
  path.join(__dirname, "..", "platforms", "win32-arm64-msvc", "package.json"),
];
const ALL_PACKAGES = [ROOT_PKG, ...PLATFORM_PKGS];

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const mode = args.find((arg) => arg !== "--dry-run") || "patch";

const SEMVER_RE =
  /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/;

function usage() {
  console.log(
    "Usage: node npm/scripts/bump-version.js [patch|minor|major|<semver>] [--dry-run]"
  );
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function writeJson(filePath, value) {
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

function readText(filePath) {
  return fs.readFileSync(filePath, "utf8");
}

function writeText(filePath, value) {
  fs.writeFileSync(filePath, value, "utf8");
}

function computeNextVersion(current, input) {
  const currentMatch = current.match(SEMVER_RE);
  if (!currentMatch) {
    throw new Error(`Current version is not valid semver: ${current}`);
  }

  if (SEMVER_RE.test(input)) {
    return input;
  }

  let major = Number(currentMatch[1]);
  let minor = Number(currentMatch[2]);
  let patch = Number(currentMatch[3]);

  if (input === "major") {
    major += 1;
    minor = 0;
    patch = 0;
    return `${major}.${minor}.${patch}`;
  }
  if (input === "minor") {
    minor += 1;
    patch = 0;
    return `${major}.${minor}.${patch}`;
  }
  if (input === "patch") {
    patch += 1;
    return `${major}.${minor}.${patch}`;
  }

  throw new Error(
    `Unsupported bump mode: ${input}. Use patch, minor, major, or an explicit semver.`
  );
}

function main() {
  const rootPkg = readJson(ROOT_PKG);
  const current = rootPkg.version;
  const next = computeNextVersion(current, mode);

  if (next === current) {
    console.log(`Version unchanged: ${current}`);
    return;
  }

  const updates = [];
  for (const filePath of ALL_PACKAGES) {
    const pkg = readJson(filePath);
    const prev = pkg.version;
    pkg.version = next;
    if (pkg.optionalDependencies) {
      for (const depName of Object.keys(pkg.optionalDependencies)) {
        pkg.optionalDependencies[depName] = next;
      }
    }
    updates.push({ filePath, prev, next, pkg });
  }

  const cargoToml = readText(CARGO_TOML);
  const cargoVersionMatch = cargoToml.match(/^version\s*=\s*"([^"]+)"\s*$/m);
  if (!cargoVersionMatch) {
    throw new Error(`Could not find package version in ${CARGO_TOML}`);
  }
  const cargoCurrent = cargoVersionMatch[1];
  const cargoNextToml = cargoToml.replace(
    /^version\s*=\s*"([^"]+)"\s*$/m,
    `version = "${next}"`
  );

  if (dryRun) {
    console.log(`[dry-run] ${current} -> ${next}`);
    for (const update of updates) {
      console.log(`[dry-run] ${path.relative(process.cwd(), update.filePath)}`);
    }
    console.log(
      `[dry-run] ${path.relative(process.cwd(), CARGO_TOML)}: ${cargoCurrent} -> ${next}`
    );
    return;
  }

  for (const update of updates) {
    writeJson(update.filePath, update.pkg);
    console.log(`${path.relative(process.cwd(), update.filePath)}: ${update.prev} -> ${update.next}`);
  }
  writeText(CARGO_TOML, cargoNextToml);
  console.log(
    `${path.relative(process.cwd(), CARGO_TOML)}: ${cargoCurrent} -> ${next}`
  );
}

try {
  main();
} catch (error) {
  usage();
  console.error(error.message);
  process.exit(1);
}
