const https = require("https");
const { createGunzip } = require("zlib");
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const PLATFORMS = {
	"darwin arm64": {
		pkg: "@bumpyclock/tasque-darwin-arm64",
		bin: "tsq",
		tuiBin: "tsq-tui",
	},
	"darwin x64": {
		pkg: "@bumpyclock/tasque-darwin-x64",
		bin: "tsq",
		tuiBin: "tsq-tui",
	},
	"linux x64": {
		pkg: "@bumpyclock/tasque-linux-x64-gnu",
		bin: "tsq",
		tuiBin: "tsq-tui",
	},
	"linux arm64": {
		pkg: "@bumpyclock/tasque-linux-arm64-gnu",
		bin: "tsq",
		tuiBin: "tsq-tui",
	},
	"win32 x64": {
		pkg: "@bumpyclock/tasque-win32-x64-msvc",
		bin: "tsq.exe",
		tuiBin: "tsq-tui.exe",
	},
	"win32 arm64": {
		pkg: "@bumpyclock/tasque-win32-arm64-msvc",
		bin: "tsq.exe",
		tuiBin: "tsq-tui.exe",
	},
};

// --- Skill refresh helpers ---

function bundledSkillsDir() {
	return path.join(__dirname, "..", "SKILLS");
}

function refreshSkills(tsqPath, options = {}) {
	const spawnSyncImpl = options.spawnSyncImpl || spawnSync;
	const baseEnv = options.env || process.env;
	const env = Object.assign({}, baseEnv, {
		TSQ_SKILLS_DIR: options.skillsDir || bundledSkillsDir(),
	});
	return spawnSyncImpl(tsqPath, ["skills", "refresh", "--json"], {
		stdio: "pipe",
		shell: false,
		// Default to 60s; tests and callers can override via options.timeout.
		timeout: options.timeout ?? 60_000,
		env,
	});
}

function shouldSkipSkillRefresh() {
	const raw = process.env.TSQ_SKIP_SKILL_REFRESH;
	return /^(1|true|yes)$/i.test(raw || "");
}

function runSkillRefreshWarnOnly(tsqPath, options = {}) {
	if (shouldSkipSkillRefresh()) return;
	try {
		const result = refreshSkills(tsqPath, options);
		if (result.error) {
			console.warn(`tasque: skill refresh failed: ${result.error.message}`);
		} else if (result.status !== 0) {
			const stdout = result.stdout ? result.stdout.toString().trim() : "";
			const stderr = result.stderr ? result.stderr.toString().trim() : "";
			let detail = stderr || stdout;
			try {
				const parsed = JSON.parse(stdout);
				if (parsed && !parsed.ok && parsed.error && parsed.error.message) {
					detail = parsed.error.message;
					if (parsed.error.details) detail += ` (${parsed.error.details})`;
				}
			} catch {
				// stdout not JSON, use raw detail
			}
			if (detail.length > 512) detail = detail.slice(0, 512) + "…";
			console.warn(
				`tasque: skill refresh exited ${result.status}${detail ? ": " + detail : ""}`,
			);
		}
	} catch (err) {
		console.warn(`tasque: skill refresh error: ${err.message}`);
	}
}

// --- Binary resolution ---

function tryResolveBinary() {
	const platformKey = `${process.platform} ${process.arch}`;
	const info = PLATFORMS[platformKey];
	if (!info) return null;

	try {
		const pkgJson = require.resolve(`${info.pkg}/package.json`);
		const binPath = path.join(path.dirname(pkgJson), info.bin);
		const tuiBinPath = path.join(path.dirname(pkgJson), info.tuiBin);
		if (fs.existsSync(binPath) && fs.existsSync(tuiBinPath)) {
			return binPath;
		}
	} catch {
		// optional dep not installed
	}
	return null;
}

function isAllowedRedirect(fromUrl, toUrl) {
	return (
		toUrl.protocol === "https:" &&
		(toUrl.hostname === fromUrl.hostname || /(^|\.)npmjs\.org$/.test(toUrl.hostname))
	);
}

function fetch(url, redirectsRemaining = 10) {
	return new Promise((resolve, reject) => {
		https
			.get(url, (res) => {
				if (
					res.statusCode >= 300 &&
					res.statusCode < 400 &&
					res.headers.location
				) {
					res.resume();
					if (redirectsRemaining <= 0) {
						return reject(new Error(`Too many redirects for ${url}`));
					}
					const currentUrl = new URL(url);
					const redirectUrl = new URL(res.headers.location, currentUrl);
					if (!isAllowedRedirect(currentUrl, redirectUrl)) {
						return reject(
							new Error(`Invalid redirect for ${url}: ${redirectUrl.toString()}`),
						);
					}
					return fetch(redirectUrl.toString(), redirectsRemaining - 1).then(
						resolve,
						reject,
					);
				}
				if (res.statusCode !== 200) {
					res.resume();
					return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
				}
				resolve(res);
			})
			.on("error", reject);
	});
}

// Minimal tar parser — extracts a single file from a tar stream (no deps).
// Intentionally does not implement GNU longlink or POSIX pax headers.
function extractFileFromTar(stream, targetName) {
	return new Promise((resolve, reject) => {
		let buf = Buffer.alloc(0);
		let found = false;
		let fileRemaining = 0;
		let fileChunks = [];
		let activeEntrySize = 0;
		let pendingPadding = 0;
		let skipping = false;

		stream.on("data", (chunk) => {
			buf = Buffer.concat([buf, chunk]);

			while (buf.length > 0) {
				// Finish discarding padding bytes from a previous chunk before parsing a header.
				if (pendingPadding > 0) {
					const take = Math.min(pendingPadding, buf.length);
					buf = buf.subarray(take);
					pendingPadding -= take;
					if (pendingPadding > 0) return;
					fileChunks = [];
					activeEntrySize = 0;
					skipping = false;
					continue;
				}

				// In skipping mode, drop non-target file bytes instead of storing them.
				if (fileRemaining > 0) {
					const take = Math.min(fileRemaining, buf.length);
					if (!skipping) {
						fileChunks.push(buf.subarray(0, take));
					}
					buf = buf.subarray(take);
					fileRemaining -= take;

					if (fileRemaining === 0) {
						// Tar file data is padded to 512-byte blocks. Consume what is
						// already buffered and keep pendingPadding for the next chunk.
						const fileData = skipping ? Buffer.alloc(0) : Buffer.concat(fileChunks);
						pendingPadding = (512 - (activeEntrySize % 512)) % 512;
						if (pendingPadding > 0) {
							const take = Math.min(pendingPadding, buf.length);
							buf = buf.subarray(take);
							pendingPadding -= take;
						}
						if (found) {
							resolve(fileData);
							stream.destroy();
							return;
						}
						if (pendingPadding === 0) {
							fileChunks = [];
							activeEntrySize = 0;
							skipping = false;
						}
					}
					continue;
				}

				// Need at least 512 bytes for a tar header
				if (buf.length < 512) return;

				const header = buf.subarray(0, 512);
				buf = buf.subarray(512);

				// End of archive starts with a zero 512-byte header block. Archives
				// usually contain two; seeing the first is enough for this extractor.
				if (header.every((b) => b === 0)) {
					break;
				}

				// Parse filename (first 100 bytes, null-terminated)
				const nameEnd = header.indexOf(0, 0);
				const name = header
					.subarray(0, Math.min(nameEnd >= 0 ? nameEnd : 100, 100))
					.toString("utf8");

				// Parse file size (octal, bytes 124-136)
				const sizeStr = header.subarray(124, 136).toString("utf8").trim();
				const size = parseInt(sizeStr, 8) || 0;

				// Strip "package/" prefix from npm tarballs
				const entryName = name.replace(/^package\//, "");

				if (entryName === targetName) {
					found = true;
					activeEntrySize = size;
					skipping = false;
					if (size === 0) {
						resolve(Buffer.alloc(0));
						stream.destroy();
						return;
					}
					fileRemaining = size;
					fileChunks = [];
				} else {
					// Skip this entry's data + padding
					const totalSize = size + ((512 - (size % 512)) % 512);
					if (buf.length >= totalSize) {
						buf = buf.subarray(totalSize);
					} else {
						fileRemaining = size;
						activeEntrySize = size;
						skipping = true;
						fileChunks = [];
					}
				}
			}
		});

		stream.on("end", () => {
			if (fileRemaining > 0 || pendingPadding > 0) {
				reject(new Error(`${targetName} tar entry truncated`));
			} else if (!found) {
				reject(new Error(`${targetName} not found in tarball`));
			}
		});

		stream.on("error", reject);
	});
}

async function downloadBinary(pkg, binName, version) {
	const parts = pkg.split("/");
	if (parts.length !== 2 || !parts[0].startsWith("@")) {
		throw new Error(`Invalid scoped package name: ${pkg}`);
	}
	const scopedName = pkg.replace("/", "%2f");
	const tarballUrl = `https://registry.npmjs.org/${scopedName}/-/${parts[1]}-${version}.tgz`;

	const binDir = path.join(__dirname, "..", "bin");
	const destPath = path.join(binDir, binName);

	const res = await fetch(tarballUrl);
	const gunzip = res.pipe(createGunzip());
	const data = await extractFileFromTar(gunzip, binName);

	fs.mkdirSync(binDir, { recursive: true });
	fs.writeFileSync(destPath, data);
	fs.chmodSync(destPath, 0o755);

	return destPath;
}

async function main() {
	let tsqPath = tryResolveBinary();

	if (tsqPath) {
		runSkillRefreshWarnOnly(tsqPath);
		return;
	}

	const platformKey = `${process.platform} ${process.arch}`;
	const info = PLATFORMS[platformKey];

	if (!info) {
		return;
	}

	const ourPkg = JSON.parse(
		fs.readFileSync(path.join(__dirname, "..", "package.json"), "utf8"),
	);
	const version = ourPkg.version;

	try {
		const dest = await downloadBinary(info.pkg, info.bin, version);
		console.log(`tasque: downloaded ${info.bin} to ${dest}`);
		const tuiDest = await downloadBinary(info.pkg, info.tuiBin, version);
		console.log(`tasque: downloaded ${info.tuiBin} to ${tuiDest}`);
		tsqPath = dest;
		runSkillRefreshWarnOnly(tsqPath);
	} catch (err) {
		console.error(
			`Error: Failed to download tsq binary for ${process.platform}-${process.arch}.\n` +
				`${err.message}\n\n` +
				`Try installing the platform package directly:\n` +
				`  npm install ${info.pkg}`,
		);
		process.exit(1);
	}
}

// --- Exports for testing ---
module.exports = {
	tryResolveBinary,
	downloadBinary,
	fetch,
	extractFileFromTar,
	bundledSkillsDir,
	refreshSkills,
	runSkillRefreshWarnOnly,
	shouldSkipSkillRefresh,
	PLATFORMS,
};

// Guard: only run main when executed directly
if (require.main === module) {
	main();
}
