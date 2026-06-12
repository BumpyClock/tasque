#!/usr/bin/env node

const { spawnSync } = require("child_process");
const path = require("path");
const { binaryNames, platformMap } = require("../scripts/postinstall");

const PLATFORMS = platformMap();

function getBinaryName() {
	return binaryNames().bin;
}

function getBinaryPath() {
	// Allow explicit override
	const override = process.env.TSQ_BINARY;
	if (override) return override;

	const platformKey = `${process.platform} ${process.arch}`;
	const info = PLATFORMS[platformKey];

	if (info) {
		// Try resolving from the optional dependency
		try {
			const pkgJson = require.resolve(`${info.pkg}/package.json`);
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
						`Tasque currently supports: ${Object.keys(PLATFORMS).join(", ")}`,
				);
			} else {
				console.error(
					`Error: Could not find the tsq binary.\n` +
						`Expected: ${bin}\n\n` +
						`Try reinstalling: npm install -g @bumpyclock/tasque`,
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
