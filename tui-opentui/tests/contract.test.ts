import { afterAll, beforeAll, describe, expect, it } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type TuiConfig, fetchDependencyTree, fetchTasks } from "../src/data";
import { readSpecLines } from "../src/tui-helpers";

const bin = process.env.TSQ_CONTRACT_BIN;

if (!bin) {
  console.log(
    "contract.test.ts: TSQ_CONTRACT_BIN not set; skipping real-binary contract tests",
  );
}

const describeContract = bin ? describe : describe.skip;

describeContract("tsq CLI contract", () => {
  let dir = "";
  let previousCwd = "";
  let epicId = "";
  let childId = "";
  let blockerId = "";

  const run = (args: string[]): string => {
    const subprocess = Bun.spawnSync([bin as string, ...args], {
      cwd: dir,
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    });
    if (subprocess.exitCode !== 0) {
      const stderr = new TextDecoder().decode(subprocess.stderr);
      throw new Error(`tsq ${args.join(" ")} failed: ${stderr}`);
    }
    return new TextDecoder().decode(subprocess.stdout);
  };

  beforeAll(() => {
    dir = mkdtempSync(join(tmpdir(), "tsq-contract-"));
    previousCwd = process.cwd();
    run(["init", "--no-wizard"]);
    const epic = JSON.parse(
      run(["create", "epic demo", "--kind", "epic", "--force", "--json"]),
    );
    epicId = epic.data.task.id;
    const child = JSON.parse(
      run(["create", "child", "--parent", epicId, "--force", "--json"]),
    );
    childId = child.data.task.id;
    const blocker = JSON.parse(run(["create", "blocker", "--force", "--json"]));
    blockerId = blocker.data.task.id;
    run(["block", childId, "by", blockerId]);
    run(["spec", childId, "--text", "# hello contract"]);
    // fetchTasks and friends spawn tsq without an explicit cwd, so run the
    // suite from inside the initialized repo, mirroring the TUI launcher.
    process.chdir(dir);
  });

  afterAll(() => {
    if (previousCwd) {
      process.chdir(previousCwd);
    }
    if (dir) {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("fetchTasks returns tasks via watch --once", async () => {
    const config: TuiConfig = {
      intervalSeconds: 2,
      statusCsv: "open,in_progress,blocked,deferred,closed,canceled",
      initialTab: "tasks",
      tsqBin: bin as string,
    };
    const snapshot = await fetchTasks(config);
    expect(snapshot.warning).toBeUndefined();
    const ids = snapshot.tasks.map((task) => task.id);
    expect(ids).toContain(epicId);
    expect(ids).toContain(childId);
    expect(ids).toContain(blockerId);
  });

  it("fetchDependencyTree returns the root via the deps verb", async () => {
    const result = await fetchDependencyTree(bin as string, childId);
    expect(result.warning).toBeUndefined();
    expect(result.root?.id).toBe(childId);
    expect(result.root?.children.length).toBeGreaterThan(0);
  });

  it("readSpecLines returns spec content via spec --show", async () => {
    const result = await readSpecLines(bin as string, childId);
    expect(result.warning).toBeUndefined();
    expect(result.lines[0]).toBe("# hello contract");
  });
});
