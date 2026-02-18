import { afterEach, describe, expect, it } from "bun:test";
import type { JsonResult } from "./helpers";
import { cleanupRepos, cliEntry, makeRepo, okData } from "./helpers";

async function runJsonSource(
  repoDir: string,
  args: string[],
  actor = "test-init-flags",
): Promise<JsonResult> {
  const proc = Bun.spawn({
    cmd: ["bun", "run", cliEntry, ...args, "--json"],
    cwd: repoDir,
    env: {
      ...process.env,
      TSQ_ACTOR: actor,
    },
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });

  const [exitCode, stdout, stderr] = await Promise.all([
    proc.exited,
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
  ]);

  const trimmed = stdout.trim();
  expect(trimmed.length > 0).toBe(true);
  const envelope = JSON.parse(trimmed);

  return {
    exitCode,
    stdout,
    stderr,
    envelope,
  };
}

afterEach(cleanupRepos);

describe("cli init flag behavior", () => {
  it("rejects --wizard in non-tty mode", async () => {
    const repo = await makeRepo("tasque-init-flags-");
    const result = await runJsonSource(repo, ["init", "--wizard"]);

    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
    expect(result.envelope.error?.message).toContain("interactive TTY");
  });

  it("rejects --preset in non-tty mode", async () => {
    const repo = await makeRepo("tasque-init-flags-");
    const result = await runJsonSource(repo, ["init", "--preset", "minimal"]);

    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
    expect(result.envelope.error?.message).toContain("interactive TTY");
  });

  it("rejects skill-scoped flags without install/uninstall when wizard disabled", async () => {
    const repo = await makeRepo("tasque-init-flags-");
    const result = await runJsonSource(repo, ["init", "--no-wizard", "--skill-targets", "codex"]);

    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
    expect(result.envelope.error?.message).toContain("skill options require --install-skill");
  });

  it("rejects --wizard with --no-wizard", async () => {
    const repo = await makeRepo("tasque-init-flags-");
    const result = await runJsonSource(repo, ["init", "--wizard", "--no-wizard"]);

    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
    expect(result.envelope.error?.message).toContain("cannot combine --wizard with --no-wizard");
  });

  it("rejects --preset with --no-wizard", async () => {
    const repo = await makeRepo("tasque-init-flags-");
    const result = await runJsonSource(repo, ["init", "--preset", "standard", "--no-wizard"]);

    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
    expect(result.envelope.error?.message).toContain("cannot combine --preset with --no-wizard");
  });

  it("accepts --yes as a no-op when wizard is disabled", async () => {
    const repo = await makeRepo("tasque-init-flags-");
    const result = await runJsonSource(repo, ["init", "--no-wizard", "--yes"]);

    expect(result.exitCode).toBe(0);
    expect(result.envelope.ok).toBe(true);
    const data = okData<{ files: string[] }>(result.envelope);
    expect(data.files).toContain(".tasque/config.json");
    expect(data.files).toContain(".tasque/events.jsonl");
    expect(data.files).toContain(".tasque/.gitignore");
  });
});
