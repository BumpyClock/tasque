const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("node:assert/strict");
const path = require("path");
const { Readable } = require("node:stream");
const {
	refreshSkills,
	runSkillRefreshWarnOnly,
	bundledSkillsDir,
	extractFileFromTar,
} = require("../scripts/postinstall");

function tarHeader(name, size) {
	const header = Buffer.alloc(512);
	header.write(name, 0, 100, "utf8");
	header.write(size.toString(8), 124, 12, "utf8");
	return header;
}

function tarArchive(entries) {
	const chunks = [];
	for (const [name, data] of entries) {
		const body = Buffer.from(data);
		chunks.push(tarHeader(name, body.length));
		chunks.push(body);
		const pad = (512 - (body.length % 512)) % 512;
		if (pad > 0) chunks.push(Buffer.alloc(pad));
	}
	chunks.push(Buffer.alloc(1024));
	return Buffer.concat(chunks);
}

function bufferStream(buffer) {
	return Readable.from([buffer]);
}

// Collect console.warn calls
function captureWarns() {
	const warns = [];
	const orig = console.warn;
	console.warn = (...args) => warns.push(args.join(" "));
	return {
		warns,
		restore: () => {
			console.warn = orig;
		},
	};
}

describe("bundledSkillsDir", () => {
	it("returns <npm-pkg-root>/SKILLS", () => {
		const dir = bundledSkillsDir();
		assert.ok(dir.endsWith(path.join("npm", "SKILLS")), `got: ${dir}`);
	});
});

describe("extractFileFromTar", () => {
	it("extracts entries with and without npm package prefix", async () => {
		const prefixed = await extractFileFromTar(
			bufferStream(tarArchive([["package/bin/tsq", "prefixed"]])),
			"bin/tsq",
		);
		assert.equal(prefixed.toString(), "prefixed");

		const plain = await extractFileFromTar(
			bufferStream(tarArchive([["tsq", "plain"]])),
			"tsq",
		);
		assert.equal(plain.toString(), "plain");
	});

	it("resolves zero-length target files", async () => {
		const data = await extractFileFromTar(
			bufferStream(tarArchive([["package/empty", ""]])),
			"empty",
		);
		assert.equal(data.length, 0);
	});

	it("continues parsing when a stream chunk ends mid-padding", async () => {
		const archive = tarArchive([
			["package/other", "abc"],
			["package/tsq", "target-data"],
		]);
		const splitAt = 512 + 3 + 10;
		const data = await extractFileFromTar(
			Readable.from([archive.subarray(0, splitAt), archive.subarray(splitAt)]),
			"tsq",
		);
		assert.equal(data.toString(), "target-data");
	});

	it("rejects missing targets after end-of-archive", async () => {
		await assert.rejects(
			() =>
				extractFileFromTar(
					bufferStream(tarArchive([["package/other", "data"]])),
					"missing",
				),
			/not found in tarball/,
		);
	});

	it("rejects truncated target entries", async () => {
		const truncated = Buffer.concat([
			tarHeader("package/tsq", 10),
			Buffer.from("abc"),
		]);
		await assert.rejects(
			() => extractFileFromTar(bufferStream(truncated), "tsq"),
			/tar entry truncated/,
		);
	});
});

describe("refreshSkills", () => {
	it("spawns tsq with skills refresh --json args", () => {
		let captured = null;
		const fakeSpawn = (cmd, args, opts) => {
			captured = { cmd, args, opts };
			return { status: 0, error: null };
		};

		refreshSkills("/path/to/tsq", { spawnSyncImpl: fakeSpawn });

		assert.deepEqual(captured.args, ["skills", "refresh", "--json"]);
		assert.equal(captured.cmd, "/path/to/tsq");
	});

	it("sets TSQ_SKILLS_DIR env via env object, not shell", () => {
		let captured = null;
		const fakeSpawn = (_cmd, _args, opts) => {
			captured = { opts };
			return { status: 0, error: null };
		};

		refreshSkills("/usr/local/bin/tsq", { spawnSyncImpl: fakeSpawn });

		assert.equal(captured.opts.shell, false, "must not use shell");
		assert.equal(captured.opts.timeout, 60_000, "must have a default timeout");
		assert.ok(captured.opts.env, "env must be set");
		assert.ok(
			captured.opts.env.TSQ_SKILLS_DIR,
			"TSQ_SKILLS_DIR must be in env",
		);
		assert.ok(
			captured.opts.env.TSQ_SKILLS_DIR.includes("SKILLS"),
			"TSQ_SKILLS_DIR should point to SKILLS dir",
		);
	});

	it("merges custom env when provided", () => {
		let captured = null;
		const fakeSpawn = (_cmd, _args, opts) => {
			captured = { opts };
			return { status: 0, error: null };
		};

		refreshSkills("/bin/tsq", {
			env: { HOME: "/tmp/home", CODEX_HOME: "/tmp/codex" },
			spawnSyncImpl: fakeSpawn,
		});

		assert.equal(captured.opts.env.HOME, "/tmp/home");
		assert.equal(captured.opts.env.CODEX_HOME, "/tmp/codex");
		assert.ok(captured.opts.env.TSQ_SKILLS_DIR.includes("SKILLS"));
	});

	it("uses custom timeout when provided", () => {
		let captured = null;
		const fakeSpawn = (_cmd, _args, opts) => {
			captured = { opts };
			return { status: 0, error: null };
		};

		refreshSkills("/bin/tsq", { spawnSyncImpl: fakeSpawn, timeout: 12_345 });

		assert.equal(captured.opts.timeout, 12_345);
	});

	it("uses custom skillsDir when provided", () => {
		let captured = null;
		const fakeSpawn = (_cmd, _args, opts) => {
			captured = { opts };
			return { status: 0, error: null };
		};

		refreshSkills("/bin/tsq", {
			spawnSyncImpl: fakeSpawn,
			skillsDir: "/custom/skills",
		});

		assert.equal(captured.opts.env.TSQ_SKILLS_DIR, "/custom/skills");
	});
});

describe("runSkillRefreshWarnOnly", () => {
	let origSkip;

	beforeEach(() => {
		origSkip = process.env.TSQ_SKIP_SKILL_REFRESH;
	});

	afterEach(() => {
		if (origSkip === undefined) {
			delete process.env.TSQ_SKIP_SKILL_REFRESH;
		} else {
			process.env.TSQ_SKIP_SKILL_REFRESH = origSkip;
		}
	});

	it("is quiet on success (status 0, no error)", () => {
		const { warns, restore } = captureWarns();
		const fakeSpawn = () => ({ status: 0, error: null });

		runSkillRefreshWarnOnly("/bin/tsq", { spawnSyncImpl: fakeSpawn });
		restore();

		assert.equal(warns.length, 0, `expected no warnings, got: ${warns}`);
	});

	it("prints warning on non-zero exit", () => {
		const { warns, restore } = captureWarns();
		const fakeSpawn = () => ({
			status: 1,
			error: null,
			stderr: Buffer.from("no such command"),
		});

		runSkillRefreshWarnOnly("/bin/tsq", { spawnSyncImpl: fakeSpawn });
		restore();

		assert.equal(warns.length, 1);
		assert.ok(warns[0].includes("skill refresh exited 1"), warns[0]);
	});

	it("prints warning on spawn error", () => {
		const { warns, restore } = captureWarns();
		const fakeSpawn = () => ({
			status: null,
			error: new Error("ENOENT: tsq not found"),
		});

		runSkillRefreshWarnOnly("/bin/tsq", { spawnSyncImpl: fakeSpawn });
		restore();

		assert.equal(warns.length, 1);
		assert.ok(warns[0].includes("skill refresh failed"), warns[0]);
	});

	it("prints warning on thrown exception", () => {
		const { warns, restore } = captureWarns();
		const fakeSpawn = () => {
			throw new Error("catastrophe");
		};

		runSkillRefreshWarnOnly("/bin/tsq", { spawnSyncImpl: fakeSpawn });
		restore();

		assert.equal(warns.length, 1);
		assert.ok(warns[0].includes("skill refresh error"), warns[0]);
	});

	it("skips refresh entirely when TSQ_SKIP_SKILL_REFRESH is truthy", () => {
		process.env.TSQ_SKIP_SKILL_REFRESH = "true";
		let spawned = false;
		const fakeSpawn = () => {
			spawned = true;
			return { status: 0 };
		};

		runSkillRefreshWarnOnly("/bin/tsq", { spawnSyncImpl: fakeSpawn });

		assert.equal(spawned, false, "should not spawn when skip is set");
	});

	it("extracts error.message from stdout JSON envelope on non-zero exit", () => {
		const { warns, restore } = captureWarns();
		const fakeSpawn = () => ({
			status: 1,
			error: null,
			stdout: Buffer.from(
				JSON.stringify({ ok: false, error: { message: "permission denied" } }),
			),
			stderr: Buffer.from(""),
		});

		runSkillRefreshWarnOnly("/bin/tsq", { spawnSyncImpl: fakeSpawn });
		restore();

		assert.equal(warns.length, 1);
		assert.ok(
			warns[0].includes("permission denied"),
			`expected 'permission denied' in: ${warns[0]}`,
		);
		assert.ok(warns[0].includes("skill refresh exited 1"), warns[0]);
	});

	it("includes error.details from stdout JSON when present", () => {
		const { warns, restore } = captureWarns();
		const fakeSpawn = () => ({
			status: 2,
			error: null,
			stdout: Buffer.from(
				JSON.stringify({
					ok: false,
					error: { message: "skill not found", details: "missing: deploy" },
				}),
			),
			stderr: Buffer.from(""),
		});

		runSkillRefreshWarnOnly("/bin/tsq", { spawnSyncImpl: fakeSpawn });
		restore();

		assert.equal(warns.length, 1);
		assert.ok(warns[0].includes("skill not found"), warns[0]);
		assert.ok(warns[0].includes("missing: deploy"), warns[0]);
	});

	it("falls back to stdout text when stderr is empty and stdout is not JSON", () => {
		const { warns, restore } = captureWarns();
		const fakeSpawn = () => ({
			status: 1,
			error: null,
			stdout: Buffer.from("some plain text error"),
			stderr: Buffer.from(""),
		});

		runSkillRefreshWarnOnly("/bin/tsq", { spawnSyncImpl: fakeSpawn });
		restore();

		assert.equal(warns.length, 1);
		assert.ok(warns[0].includes("some plain text error"), warns[0]);
	});

	it("prefers stderr over stdout when both present and stdout is not valid JSON error", () => {
		const { warns, restore } = captureWarns();
		const fakeSpawn = () => ({
			status: 1,
			error: null,
			stdout: Buffer.from("not json"),
			stderr: Buffer.from("stderr message"),
		});

		runSkillRefreshWarnOnly("/bin/tsq", { spawnSyncImpl: fakeSpawn });
		restore();

		assert.equal(warns.length, 1);
		assert.ok(warns[0].includes("stderr message"), warns[0]);
	});

	it("does not throw even on failure (warn-only)", () => {
		delete process.env.TSQ_SKIP_SKILL_REFRESH;
		const { warns, restore } = captureWarns();
		const fakeSpawn = () => ({
			status: 127,
			error: null,
			stderr: Buffer.from("command not found"),
		});

		// Must not throw
		assert.doesNotThrow(() => {
			runSkillRefreshWarnOnly("/bin/tsq", { spawnSyncImpl: fakeSpawn });
		});
		restore();

		assert.ok(warns.length > 0, "should have warned");
	});
});
