import { afterEach, describe, expect, it } from "bun:test";
import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { cleanupRepos, makeRepo as makeRepoBase, okData, runCli as runCliBase, runJson as runJsonBase } from "./helpers";

interface TaskNote {
  event_id: string;
  ts: string;
  actor: string;
  text: string;
}

async function makeRepo(): Promise<string> {
  return makeRepoBase("tasque-rich-content-");
}

afterEach(cleanupRepos);

async function runJson(repoDir: string, args: string[], actor = "test-rich-content", stdinText?: string) {
  return runJsonBase(repoDir, args, actor, stdinText);
}

async function runCli(repoDir: string, args: string[]) {
  return runCliBase(repoDir, args, "test-rich-content");
}

describe("cli rich content", () => {
  it("create stores description and show returns empty notes list", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{
      task: { id: string; description?: string; notes: TaskNote[] };
    }>(
      (await runJson(repo, ["create", "Rich task", "--description", "Capture rollout context"]))
        .envelope,
    ).task;

    expect(created.description).toBe("Capture rollout context");
    expect(created.notes).toEqual([]);

    const shown = okData<{
      task: { id: string; description?: string; notes: TaskNote[] };
    }>((await runJson(repo, ["show", created.id])).envelope);
    expect(shown.task.description).toBe("Capture rollout context");
    expect(shown.task.notes).toEqual([]);
  });

  it("update sets and clears description", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Description flow"])).envelope,
    ).task;

    const updated = await runJson(repo, [
      "update",
      created.id,
      "--description",
      "Detailed implementation note",
    ]);
    expect(updated.exitCode).toBe(0);

    const shownWithDescription = okData<{ task: { description?: string } }>(
      (await runJson(repo, ["show", created.id])).envelope,
    );
    expect(shownWithDescription.task.description).toBe("Detailed implementation note");

    const cleared = await runJson(repo, ["update", created.id, "--clear-description"]);
    expect(cleared.exitCode).toBe(0);

    const shownCleared = okData<{ task: { description?: string } }>(
      (await runJson(repo, ["show", created.id])).envelope,
    );
    expect(shownCleared.task.description).toBeUndefined();
  });

  it("rejects combining --description with --clear-description", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Conflict task"])).envelope,
    ).task;

    const result = await runJson(repo, [
      "update",
      created.id,
      "--description",
      "A",
      "--clear-description",
    ]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("keeps claim mode exclusive from description options", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Claim conflict task"])).envelope,
    ).task;

    const result = await runJson(repo, [
      "update",
      created.id,
      "--claim",
      "--description",
      "should fail",
    ]);
    expect(result.exitCode).toBe(1);
    expect(result.envelope.ok).toBe(false);
    expect(result.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("note add appends deterministic metadata and note list returns all entries", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Note flow"])).envelope,
    ).task;

    const first = await runJson(repo, ["note", "add", created.id, "First note body"], "actor-one");
    expect(first.exitCode).toBe(0);
    const firstData = okData<{ task_id: string; note: TaskNote; notes_count: number }>(
      first.envelope,
    );
    expect(firstData.task_id).toBe(created.id);
    expect(firstData.note.actor).toBe("actor-one");
    expect(firstData.note.text).toBe("First note body");
    expect(firstData.note.event_id.length > 0).toBe(true);
    expect(firstData.note.ts.length > 0).toBe(true);
    expect(firstData.notes_count).toBe(1);

    await runJson(repo, ["note", "add", created.id, "Second note body"], "actor-two");

    const listed = await runJson(repo, ["note", "list", created.id]);
    expect(listed.exitCode).toBe(0);
    const listData = okData<{ task_id: string; notes: TaskNote[] }>(listed.envelope);
    expect(listData.task_id).toBe(created.id);
    expect(listData.notes.length).toBe(2);
    expect(listData.notes[0]?.text).toBe("First note body");
    expect(listData.notes[1]?.text).toBe("Second note body");
    expect(listData.notes[1]?.actor).toBe("actor-two");
  });

  it("show human render includes description and notes visibility", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);

    const created = okData<{ task: { id: string } }>(
      (
        await runJson(repo, [
          "create",
          "Human render rich task",
          "--description",
          "Visible description text",
        ])
      ).envelope,
    ).task;
    await runJson(repo, ["note", "add", created.id, "Visible note"]);

    const shown = await runCli(repo, ["show", created.id]);
    expect(shown.exitCode).toBe(0);
    expect(shown.stdout.includes("description=Visible description text")).toBe(true);
    expect(shown.stdout.includes("notes=1")).toBe(true);
  });

  it("spec attach stores markdown from --text and show exposes spec metadata", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Spec attach text"])).envelope,
    ).task;

    const markdown = "# Delivery Plan\n\n- one\n- two\n";
    const attached = await runJson(repo, ["spec", "attach", created.id, "--text", markdown]);
    expect(attached.exitCode).toBe(0);
    const attachedData = okData<{
      task: {
        id: string;
        spec_path?: string;
        spec_fingerprint?: string;
        spec_attached_at?: string;
        spec_attached_by?: string;
      };
      spec: {
        spec_path: string;
        spec_fingerprint: string;
        spec_attached_at: string;
        spec_attached_by: string;
        bytes: number;
      };
    }>(attached.envelope);

    expect(attachedData.spec.spec_path).toBe(`.tasque/specs/${created.id}/spec.md`);
    expect(attachedData.task.spec_path).toBe(attachedData.spec.spec_path);
    expect(attachedData.task.spec_fingerprint).toBe(attachedData.spec.spec_fingerprint);
    expect(attachedData.task.spec_attached_at).toBe(attachedData.spec.spec_attached_at);
    expect(attachedData.task.spec_attached_by).toBe("test-rich-content");
    expect(attachedData.spec.bytes).toBe(markdown.length);

    const persisted = await readFile(join(repo, ".tasque", "specs", created.id, "spec.md"), "utf8");
    expect(persisted).toBe(markdown);
    expect(attachedData.spec.spec_fingerprint).toBe(
      createHash("sha256").update(persisted, "utf8").digest("hex"),
    );

    const shown = okData<{
      task: {
        spec_path?: string;
        spec_fingerprint?: string;
        spec_attached_at?: string;
        spec_attached_by?: string;
      };
    }>((await runJson(repo, ["show", created.id])).envelope);
    expect(shown.task.spec_path).toBe(`.tasque/specs/${created.id}/spec.md`);
    expect(shown.task.spec_fingerprint).toBe(attachedData.spec.spec_fingerprint);
    expect(shown.task.spec_attached_at).toBe(attachedData.spec.spec_attached_at);
    expect(shown.task.spec_attached_by).toBe(attachedData.spec.spec_attached_by);

    const shownHuman = await runCli(repo, ["show", created.id]);
    expect(shownHuman.exitCode).toBe(0);
    expect(shownHuman.stdout.includes(`spec=.tasque/specs/${created.id}/spec.md`)).toBe(true);
  });

  it("spec attach supports positional file shorthand and stdin", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const fromFileTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Spec attach file"])).envelope,
    ).task;

    const sourceFile = join(repo, "spec-source.md");
    await writeFile(sourceFile, "## Source file spec\n", "utf8");
    const fileAttach = await runJson(repo, ["spec", "attach", fromFileTask.id, sourceFile]);
    expect(fileAttach.exitCode).toBe(0);
    const fileAttachData = okData<{ spec: { spec_path: string } }>(fileAttach.envelope);
    expect(fileAttachData.spec.spec_path).toBe(`.tasque/specs/${fromFileTask.id}/spec.md`);
    expect(await readFile(join(repo, ".tasque", "specs", fromFileTask.id, "spec.md"), "utf8")).toBe(
      "## Source file spec\n",
    );

    const fromStdinTask = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Spec attach stdin"])).envelope,
    ).task;
    const stdinAttach = await runJson(
      repo,
      ["spec", "attach", fromStdinTask.id, "--stdin"],
      "stdin-actor",
      "### stdin spec\n",
    );
    expect(stdinAttach.exitCode).toBe(0);
    const stdinAttachData = okData<{
      task: { spec_attached_by?: string };
      spec: { spec_attached_by: string };
    }>(stdinAttach.envelope);
    expect(stdinAttachData.task.spec_attached_by).toBe("stdin-actor");
    expect(stdinAttachData.spec.spec_attached_by).toBe("stdin-actor");
    expect(
      await readFile(join(repo, ".tasque", "specs", fromStdinTask.id, "spec.md"), "utf8"),
    ).toBe("### stdin spec\n");
  });

  it("spec attach enforces exactly one source and non-empty markdown", async () => {
    const repo = await makeRepo();
    await runJson(repo, ["init"]);
    const created = okData<{ task: { id: string } }>(
      (await runJson(repo, ["create", "Spec attach validation"])).envelope,
    ).task;
    const sourceFile = join(repo, "spec-source-validation.md");
    await writeFile(sourceFile, "# validation\n", "utf8");

    const conflict = await runJson(repo, [
      "spec",
      "attach",
      created.id,
      "--file",
      sourceFile,
      "--text",
      "# inline",
    ]);
    expect(conflict.exitCode).toBe(1);
    expect(conflict.envelope.ok).toBe(false);
    expect(conflict.envelope.error?.code).toBe("VALIDATION_ERROR");

    const empty = await runJson(repo, ["spec", "attach", created.id, "--text", " \n\t "]);
    expect(empty.exitCode).toBe(1);
    expect(empty.envelope.ok).toBe(false);
    expect(empty.envelope.error?.code).toBe("VALIDATION_ERROR");

    const missingTask = await runJson(repo, ["spec", "attach", "tsq-missing", "--text", "# x"]);
    expect(missingTask.exitCode).toBe(1);
    expect(missingTask.envelope.ok).toBe(false);
    expect(missingTask.envelope.error?.code).toBe("TASK_NOT_FOUND");
  });
});
