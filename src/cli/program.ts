import { Command } from "commander";
import { normalizeStatus, parsePriority } from "../app/runtime";
import type { TasqueService } from "../app/service";
import { TsqError } from "../errors";
import { errEnvelope, okEnvelope } from "../output";
import type { RelationType, TaskKind, TaskStatus } from "../types";
import { printTask, printTaskList } from "./render";

interface GlobalOpts {
  json?: boolean;
  exactId?: boolean;
}

interface RuntimeDeps {
  service: TasqueService;
}

export function buildProgram(deps: RuntimeDeps): Command {
  const program = new Command();
  program
    .name("tsq")
    .description("Local durable task graph for coding agents")
    .option("--json", "emit JSON envelope")
    .option("--exact-id", "require exact task ID match");

  program
    .command("init")
    .description("Initialize .tasque storage")
    .action(async function action() {
      await runAction(this, async () => deps.service.init(), {
        jsonData: (data) => data,
        human: (data) => {
          for (const file of data.files) {
            console.log(`created ${file}`);
          }
        },
      });
    });

  program
    .command("create")
    .argument("<title>", "task title")
    .option("--kind <kind>", "task kind: task|feature|epic", "task")
    .option("-p, --priority <priority>", "priority 0..3", "2")
    .option("--parent <id>", "parent task ID")
    .description("Create task")
    .action(async function action(title: string, options: Record<string, string>) {
      await runAction(
        this,
        async (opts) => {
          const kind = parseKind(options.kind ?? "task");
          const priority = parsePriority(options.priority ?? "2");
          return deps.service.create({
            title,
            kind,
            priority,
            parent: options.parent,
            exactId: opts.exactId,
          });
        },
        {
          jsonData: (task) => ({ task }),
          human: (task) => printTask(task),
        },
      );
    });

  program
    .command("show")
    .argument("<id>", "task id")
    .description("Show task details")
    .action(async function action(id: string) {
      await runAction(this, async (opts) => deps.service.show(id, opts.exactId), {
        jsonData: (data) => data,
        human: (data) => {
          printTask(data.task);
          if (data.blockers.length > 0) {
            console.log(`blockers=${data.blockers.join(",")}`);
          }
          if (data.dependents.length > 0) {
            console.log(`dependents=${data.dependents.join(",")}`);
          }
          console.log(`ready=${data.ready}`);
          if (Object.keys(data.links).length > 0) {
            console.log(`links=${JSON.stringify(data.links)}`);
          }
          if (data.history.length > 0) {
            console.log(`history_events=${data.history.length}`);
          }
        },
      });
    });

  program
    .command("list")
    .option("--status <status>", "filter by status")
    .option("--assignee <assignee>", "filter by assignee")
    .option("--kind <kind>", "filter by kind")
    .description("List tasks")
    .action(async function action(options: Record<string, string | undefined>) {
      await runAction(
        this,
        async () => {
          const filter: {
            status?: TaskStatus;
            assignee?: string;
            kind?: TaskKind;
          } = {};
          if (options.status) {
            filter.status = normalizeStatus(options.status);
          }
          if (options.assignee) {
            filter.assignee = options.assignee;
          }
          if (options.kind) {
            filter.kind = parseKind(options.kind);
          }
          return deps.service.list(filter);
        },
        {
          jsonData: (tasks) => ({ tasks }),
          human: (tasks) => printTaskList(tasks),
        },
      );
    });

  program
    .command("ready")
    .description("List ready tasks")
    .action(async function action() {
      await runAction(this, async () => deps.service.ready(), {
        jsonData: (tasks) => ({ tasks }),
        human: (tasks) => printTaskList(tasks),
      });
    });

  program
    .command("doctor")
    .description("Validate local tasque store health")
    .action(async function action() {
      await runAction(this, async () => deps.service.doctor(), {
        jsonData: (data) => data,
        human: (data) => {
          console.log(
            `tasks=${data.tasks} events=${data.events} snapshot_loaded=${data.snapshot_loaded}`,
          );
          if (data.warning) {
            console.log(`warning=${data.warning}`);
          }
          if (data.issues.length === 0) {
            console.log("issues=none");
          } else {
            for (const issue of data.issues) {
              console.log(`issue=${issue}`);
            }
          }
        },
      });
    });

  program
    .command("update")
    .argument("<id>", "task id")
    .option("--title <title>", "new title")
    .option("--status <status>", "status value")
    .option("--priority <priority>", "priority 0..3")
    .option("--claim", "claim this task")
    .option("--assignee <assignee>", "assignee for claim")
    .description("Update task")
    .action(async function action(
      id: string,
      options: Record<string, string | boolean | undefined>,
    ) {
      await runAction(
        this,
        async (opts) => {
          const claim = Boolean(options.claim);
          if (claim) {
            if (options.title || options.status || options.priority) {
              throw new TsqError(
                "VALIDATION_ERROR",
                "cannot combine --claim with --title/--status/--priority",
                1,
              );
            }
            return deps.service.claim({
              id,
              assignee: asOptionalString(options.assignee),
              exactId: opts.exactId,
            });
          }
          return deps.service.update({
            id,
            title: asOptionalString(options.title),
            status: options.status ? normalizeStatus(String(options.status)) : undefined,
            priority: options.priority ? parsePriority(String(options.priority)) : undefined,
            exactId: opts.exactId,
          });
        },
        {
          jsonData: (task) => ({ task }),
          human: (task) => printTask(task),
        },
      );
    });

  const dep = program.command("dep").description("Dependency operations");
  dep
    .command("add")
    .argument("<child>", "child task")
    .argument("<blocker>", "blocker task")
    .description("Add dependency")
    .action(async function action(child: string, blocker: string) {
      await runAction(
        this,
        async (opts) => deps.service.depAdd({ child, blocker, exactId: opts.exactId }),
        {
          jsonData: (data) => data,
          human: (data) => console.log(`added dep ${data.child} -> ${data.blocker}`),
        },
      );
    });

  dep
    .command("remove")
    .argument("<child>", "child task")
    .argument("<blocker>", "blocker task")
    .description("Remove dependency")
    .action(async function action(child: string, blocker: string) {
      await runAction(
        this,
        async (opts) => deps.service.depRemove({ child, blocker, exactId: opts.exactId }),
        {
          jsonData: (data) => data,
          human: (data) => console.log(`removed dep ${data.child} -> ${data.blocker}`),
        },
      );
    });

  const link = program.command("link").description("Task relation links");
  link
    .command("add")
    .argument("<src>", "source task")
    .argument("<dst>", "destination task")
    .requiredOption("--type <type>", "relates_to|replies_to|duplicates|supersedes")
    .description("Add relation")
    .action(async function action(src: string, dst: string, options: { type: string }) {
      await runAction(
        this,
        async (opts) =>
          deps.service.linkAdd({
            src,
            dst,
            type: parseRelationType(options.type),
            exactId: opts.exactId,
          }),
        {
          jsonData: (data) => data,
          human: (data) => console.log(`added link ${data.type}: ${data.src} -> ${data.dst}`),
        },
      );
    });

  link
    .command("remove")
    .argument("<src>", "source task")
    .argument("<dst>", "destination task")
    .requiredOption("--type <type>", "relates_to|replies_to|duplicates|supersedes")
    .description("Remove relation")
    .action(async function action(src: string, dst: string, options: { type: string }) {
      await runAction(
        this,
        async (opts) =>
          deps.service.linkRemove({
            src,
            dst,
            type: parseRelationType(options.type),
            exactId: opts.exactId,
          }),
        {
          jsonData: (data) => data,
          human: (data) => console.log(`removed link ${data.type}: ${data.src} -> ${data.dst}`),
        },
      );
    });

  program
    .command("supersede")
    .argument("<old-id>", "source task id")
    .requiredOption("--with <new-id>", "replacement task id")
    .option("--reason <text>", "supersede reason")
    .description("Supersede a task with another")
    .action(async function action(oldId: string, options: { with: string; reason?: string }) {
      await runAction(
        this,
        async (opts) =>
          deps.service.supersede({
            source: oldId,
            withId: options.with,
            reason: options.reason,
            exactId: opts.exactId,
          }),
        {
          jsonData: (task) => ({ task }),
          human: (task) => printTask(task),
        },
      );
    });

  return program;
}

interface ActionRender<TValue, TJson> {
  jsonData: (value: TValue) => TJson;
  human: (value: TValue) => void;
}

async function runAction<TValue, TJson>(
  command: Command,
  action: (opts: GlobalOpts) => Promise<TValue>,
  render: ActionRender<TValue, TJson>,
): Promise<void> {
  const commandLine = commandPath(command);
  const options = command.optsWithGlobals<GlobalOpts>();
  try {
    const value = await action(options);
    if (options.json) {
      console.log(JSON.stringify(okEnvelope(commandLine, render.jsonData(value)), null, 2));
      return;
    }
    render.human(value);
  } catch (error) {
    const tsqError = asTsqError(error);
    if (options.json) {
      console.log(
        JSON.stringify(
          errEnvelope(commandLine, tsqError.code, tsqError.message, tsqError.details),
          null,
          2,
        ),
      );
    } else {
      console.error(`${tsqError.code}: ${tsqError.message}`);
      if (tsqError.details) {
        console.error(JSON.stringify(tsqError.details));
      }
    }
    process.exitCode = tsqError.exitCode;
  }
}

function commandPath(command: Command): string {
  const names: string[] = [];
  let cursor: Command | null = command;
  while (cursor) {
    names.push(cursor.name());
    cursor = cursor.parent ?? null;
  }
  return names.reverse().join(" ");
}

function parseKind(raw: string): TaskKind {
  if (raw === "task" || raw === "feature" || raw === "epic") {
    return raw;
  }
  throw new TsqError("VALIDATION_ERROR", "kind must be task|feature|epic", 1);
}

function parseRelationType(raw: string): RelationType {
  if (
    raw === "relates_to" ||
    raw === "replies_to" ||
    raw === "duplicates" ||
    raw === "supersedes"
  ) {
    return raw;
  }
  throw new TsqError(
    "VALIDATION_ERROR",
    "relation type must be relates_to|replies_to|duplicates|supersedes",
    1,
  );
}

function asTsqError(error: unknown): TsqError {
  if (error instanceof TsqError) {
    return error;
  }
  const message = error instanceof Error ? error.message : "unexpected error";
  return new TsqError("INTERNAL_ERROR", message, 2);
}

function asOptionalString(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}
