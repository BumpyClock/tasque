#!/usr/bin/env bun
import { resolve } from "node:path";
import { Glob } from "bun";
import pc from "picocolors";

interface TestResult {
  file: string;
  exitCode: number;
  stdout: string;
  stderr: string;
  durationMs: number;
}

export async function discoverTestFiles(rootDir: string): Promise<string[]> {
  const pattern = "tests/**/*.test.ts";
  const g = new Glob(pattern);
  const files: string[] = [];
  for await (const file of g.scan({ cwd: rootDir, absolute: true })) {
    files.push(resolve(rootDir, file));
  }
  return files;
}

export async function runTestFile(file: string): Promise<TestResult> {
  const start = Date.now();

  const proc = Bun.spawn(["bun", "test", file], {
    stdout: "pipe",
    stderr: "pipe",
  });

  const [stdout, stderr, exitCode] = await Promise.all([
    proc.stdout.text(),
    proc.stderr.text(),
    proc.exited,
  ]);

  const durationMs = Date.now() - start;

  return {
    file,
    exitCode,
    stdout,
    stderr,
    durationMs,
  };
}

export async function runPool<T, R>(
  items: T[],
  concurrency: number,
  fn: (item: T) => Promise<R>,
): Promise<R[]> {
  const results: R[] = [];
  const executing: Set<Promise<void>> = new Set();

  for (const item of items) {
    const promise = fn(item)
      .then((result) => {
        results.push(result);
      })
      .finally(() => {
        executing.delete(promise);
      });

    executing.add(promise);

    if (executing.size >= concurrency) {
      await Promise.race(executing);
    }
  }

  await Promise.all(executing);

  return results;
}

export function formatResults(results: TestResult[]): string {
  const passed = results.filter((r) => r.exitCode === 0).length;
  const failed = results.filter((r) => r.exitCode !== 0).length;
  const total = results.length;

  let output = "\n";

  for (const result of results) {
    const status = result.exitCode === 0 ? pc.green("PASS") : pc.red("FAIL");
    const duration = pc.dim(`${result.durationMs}ms`);
    const fileName = result.file.split(/[/\\]/).pop() || result.file;
    output += `${status} ${fileName} ${duration}\n`;
  }

  output += "\n";
  output += pc.bold("Summary:\n");
  output += `  Total:  ${total}\n`;
  output += `  ${pc.green("Passed:")} ${passed}\n`;
  output += `  ${pc.red("Failed:")} ${failed}\n`;

  return output;
}

async function main() {
  const repoRoot = resolve(import.meta.dir, "..");
  const startTime = Date.now();

  console.log(pc.bold("Discovering test files...\n"));
  const testFiles = await discoverTestFiles(repoRoot);

  if (testFiles.length === 0) {
    console.log(pc.yellow("No test files found"));
    process.exit(0);
  }

  console.log(pc.dim(`Found ${testFiles.length} test files\n`));
  console.log(pc.bold("Running tests...\n"));

  const results = await runPool(testFiles, 4, runTestFile);

  for (const result of results) {
    if (result.stdout) {
      console.log(result.stdout);
    }
    if (result.stderr) {
      console.error(result.stderr);
    }
  }

  const summary = formatResults(results);
  console.log(summary);

  const totalTime = Date.now() - startTime;
  console.log(pc.dim(`Total time: ${totalTime}ms\n`));

  const hasFailures = results.some((r) => r.exitCode !== 0);
  if (hasFailures) {
    console.log(pc.red(pc.bold("\nFailures detected:\n")));
    for (const result of results.filter((r) => r.exitCode !== 0)) {
      const fileName = result.file.split(/[/\\]/).pop() || result.file;
      console.log(pc.red(`\n=== ${fileName} ===`));
      if (result.stdout) console.log(result.stdout);
      if (result.stderr) console.error(result.stderr);
    }
    process.exit(1);
  }
}

if (import.meta.main) {
  main();
}
