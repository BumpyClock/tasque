#!/usr/bin/env node

"use strict";

const { spawnSync } = require("child_process");
const path = require("path");

const PLATFORMS = {
  "darwin arm64": "@bumpyclock/tasque-darwin-arm64",
  "darwin x64": "@bumpyclock/tasque-darwin-x64",
  "linux x64": "@bumpyclock/tasque-linux-x64-gnu",
  "linux arm64": "@bumpyclock/tasque-linux-arm64-gnu",
  "win32 x64": "@bumpyclock/tasque-win32-x64-msvc",
  "win32 arm64": "@bumpyclock/tasque-win32-arm64-msvc",
};

function getBinaryName() {
  return process.platform === "win32" ? "tsq.exe" : "tsq";
}

function getBinaryPath() {
  // Allow explicit override
  const override = process.env.TSQ_BINARY;
  if (override) return override;

  const platformKey = `${process.platform} ${process.arch}`;
  const pkg = PLATFORMS[platformKey];

  if (pkg) {
    // Try resolving from the optional dependency
    try {
      const pkgJson = require.resolve(`${pkg}/package.json`);
      return path.join(path.dirname(pkgJson), getBinaryName());
    } catch {
      // Optional dep not installed — fall through to postinstall fallback
    }
  }

  // Fallback: postinstall may have placed the binary alongside this script
  const fallback = path.join(__dirname, getBinaryName());
  return fallback;
}

function main() {
  const bin = getBinaryPath();
  const result = spawnSync(bin, process.argv.slice(2), {
    stdio: "inherit",
    shell: false,
  });

  if (result.error) {
    if (result.error.code === "ENOENT") {
      const platformKey = `${process.platform} ${process.arch}`;
      if (!PLATFORMS[platformKey]) {
        console.error(
          `Error: Unsupported platform ${process.platform} ${process.arch}.\n` +
            `Tasque currently supports: ${Object.keys(PLATFORMS).join(", ")}`
        );
      } else {
        console.error(
          `Error: Could not find the tsq binary.\n` +
            `Expected: ${bin}\n\n` +
            `Try reinstalling: npm install -g @bumpyclock/tasque`
        );
      }
    } else {
      console.error(`Error: ${result.error.message}`);
    }
    process.exit(1);
  }

  process.exit(result.status ?? 1);
}

main();
