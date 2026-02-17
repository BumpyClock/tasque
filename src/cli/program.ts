import { Command } from "commander";
import { normalizeStatus, parsePriority } from "../app/runtime";
import type { ListFilter, TasqueService } from "../app/service";
import type { DepDirection } from "../domain/dep-tree";
import { TsqError } from "../errors";
import { errEnvelope, okEnvelope } from "../output";
import type { SkillTarget } from "../skills/types";
import type { RelationType, TaskKind, TaskStatus } from "../types";
import {
  printDepTreeResult,
  printHistory,
  printLabelList,
  printRepairResult,
  printTask,
  printTaskList,
  printTaskTree,
} from "./render";

interface GlobalOpts {
  json?: boolean;
  exactId?: boolean;
}

interface RuntimeDeps {
  service: TasqueService;
}

interface InitCommandOptions {
  installSkill?: boolean;
  uninstallSkill?: boolean;
  skillTargets?: string;
  skillName?: string;
  forceSkillOverwrite?: boolean;
  skillDirClaude?: string;
  skillDirCodex?: string;
  skillDirCopilot?: string;
  skillDirOpencode?: string;
}

interface ListCommandOptions {
  status?: string;
  assignee?: string;
  kind?: string;
  label?: string;
  tree?: boolean;
  full?: boolean;
}

const TREE_DEFAULT_STATUSES: TaskStatus[] = ["open", "in_progress"];

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
    .option("--install-skill", "install tsq skill files")
    .option("--uninstall-skill", "uninstall tsq skill files")
    .option(
      "--skill-targets <targets>",
      "comma-separated: claude,codex,copilot,opencode,all",
      "all",
    )
    .option("--skill-name <name>", "skill folder name", "tasque")
    .option("--force-skill-overwrite", "overwrite unmanaged skill files")
    .option("--skill-dir-claude <path>", "override claude skills root dir")
    .option("--skill-dir-codex <path>", "override codex skills root dir")
    .option("--skill-dir-copilot <path>", "override copilot skills root dir")
    .option("--skill-dir-opencode <path>", "override opencode skills root dir")
    .action(async function action(initOptions: InitCommandOptions) {
      await runAction(
        this,
        async () => {
          const wantsSkillOperation = Boolean(
            initOptions.installSkill || initOptions.uninstallSkill,
          );
          return deps.service.init({
            installSkill: Boolean(initOptions.installSkill),
            uninstallSkill: Boolean(initOptions.uninstallSkill),
            skillTargets: wantsSkillOperation
              ? parseSkillTargets(initOptions.skillTargets ?? "all")
              : undefined,
            skillName: wantsSkillOperation
              ? (asOptionalString(initOptions.skillName) ?? "tasque")
              : undefined,
            forceSkillOverwrite: Boolean(initOptions.forceSkillOverwrite),
            skillDirClaude: asOptionalString(initOptions.skillDirClaude),
            skillDirCodex: asOptionalString(initOptions.skillDirCodex),
            skillDirCopilot: asOptionalString(initOptions.skillDirCopilot),
            skillDirOpencode: asOptionalString(initOptions.skillDirOpencode),
          });
        },
        {
          jsonData: (data) => data,
          human: (data) => {
            for (const file of data.files) {
              console.log(`created ${file}`);
            }
            if (data.skill_operation) {
              for (const result of data.skill_operation.results) {
                const message = result.message ? ` ${result.message}` : "";
                console.log(`skill ${result.target} ${result.status} ${result.path}${message}`);
              }
            }
          },
        },
      );
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
    .option("--label <label>", "filter by label")
    .option("--tree", "render parent/child hierarchy")
    .option("--full", "with --tree, include closed/blocked/canceled items")
    .description("List tasks")
    .action(async function action(options: ListCommandOptions) {
      const filter = parseListFilter(options);
      if (options.tree) {
        await runAction(
          this,
          async () => deps.service.listTree(applyTreeDefaults(filter, options)),
          {
            jsonData: (tree) => ({ tree }),
            human: (tree) => printTaskTree(tree),
          },
        );
        return;
      }

      await runAction(
        this,
        async () => {
          if (options.full) {
            throw new TsqError("VALIDATION_ERROR", "--full requires --tree", 1);
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
    .command("repair")
    .description("Audit and fix integrity issues (dry-run by default)")
    .option("--fix", "Apply repairs (default is dry-run)")
    .option("--force-unlock", "Force-remove stale lock file (requires --fix)")
    .action(async function action(options: { fix?: boolean; forceUnlock?: boolean }) {
      await runAction(
        this,
        async () => {
          return deps.service.repair({
            fix: Boolean(options.fix),
            forceUnlock: Boolean(options.forceUnlock),
          });
        },
        {
          jsonData: (data) => data,
          human: (data) => printRepairResult(data),
        },
      );
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

  program
    .command("close")
    .argument("<ids...>", "task ids to close")
    .option("--reason <text>", "close reason")
    .description("Close tasks")
    .action(async function action(ids: string[], options: { reason?: string }) {
      await runAction(
        this,
        async (opts) => deps.service.close({ ids, reason: options.reason, exactId: opts.exactId }),
        {
          jsonData: (tasks) => (tasks.length === 1 ? { task: tasks[0] } : { tasks }),
          human: (tasks) => {
            for (const task of tasks) {
              printTask(task);
            }
          },
        },
      );
    });

  program
    .command("reopen")
    .argument("<ids...>", "task ids to reopen")
    .description("Reopen closed tasks")
    .action(async function action(ids: string[]) {
      await runAction(this, async (opts) => deps.service.reopen({ ids, exactId: opts.exactId }), {
        jsonData: (tasks) => (tasks.length === 1 ? { task: tasks[0] } : { tasks }),
        human: (tasks) => {
          for (const task of tasks) {
            printTask(task);
          }
        },
      });
    });

  program
    .command("history")
    .argument("<id>", "task id")
    .option("--limit <n>", "max events to show")
    .option("--type <type>", "filter by event type")
    .option("--actor <name>", "filter by actor")
    .option("--since <iso>", "filter events after this date")
    .description("Show task history")
    .action(async function action(
      id: string,
      options: { limit?: string; type?: string; actor?: string; since?: string },
    ) {
      await runAction(
        this,
        async (opts) =>
          deps.service.history({
            id,
            limit: options.limit ? Number.parseInt(options.limit, 10) : undefined,
            type: options.type,
            actor: options.actor,
            since: options.since,
            exactId: opts.exactId,
          }),
        {
          jsonData: (data) => data,
          human: (data) => printHistory(data),
        },
      );
    });

  const label = program.command("label").description("Label operations");
  label
    .command("add")
    .argument("<id>", "task id")
    .argument("<label>", "label to add")
    .description("Add label to task")
    .action(async function action(id: string, labelStr: string) {
      await runAction(
        this,
        async (opts) => deps.service.labelAdd({ id, label: labelStr, exactId: opts.exactId }),
        {
          jsonData: (task) => ({ task }),
          human: (task) => printTask(task),
        },
      );
    });

  label
    .command("remove")
    .argument("<id>", "task id")
    .argument("<label>", "label to remove")
    .description("Remove label from task")
    .action(async function action(id: string, labelStr: string) {
      await runAction(
        this,
        async (opts) => deps.service.labelRemove({ id, label: labelStr, exactId: opts.exactId }),
        {
          jsonData: (task) => ({ task }),
          human: (task) => printTask(task),
        },
      );
    });

  label
    .command("list")
    .description("List all labels with counts")
    .action(async function action() {
      await runAction(this, async () => deps.service.labelList(), {
        jsonData: (data) => ({ labels: data }),
        human: (data) => printLabelList(data),
      });
    });

  dep
    .command("tree")
    .argument("<id>", "root task id")
    .option("--direction <dir>", "up|down|both", "both")
    .option("--depth <n>", "max depth")
    .description("Show dependency tree")
    .action(async function action(id: string, options: { direction?: string; depth?: string }) {
      await runAction(
        this,
        async (opts) =>
          deps.service.depTree({
            id,
            direction: parseDepDirection(options.direction),
            depth: options.depth ? Number.parseInt(options.depth, 10) : undefined,
            exactId: opts.exactId,
          }),
        {
          jsonData: (root) => ({ root }),
          human: (root) => printDepTreeResult(root),
        },
      );
    });

  program
    .command("search")
    .argument("<query>", "search query")
    .description("Search tasks")
    .action(async function action(query: string) {
      await runAction(this, async () => deps.service.search({ query }), {
        jsonData: (tasks) => ({ tasks }),
        human: (tasks) => printTaskList(tasks),
      });
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

function parseSkillTargets(raw: string): SkillTarget[] {
  const tokens = raw
    .split(",")
    .map((entry) => entry.trim().toLowerCase())
    .filter((entry) => entry.length > 0);
  if (tokens.length === 0) {
    throw new TsqError("VALIDATION_ERROR", "skill targets must not be empty", 1);
  }

  const validTargets: SkillTarget[] = ["claude", "codex", "copilot", "opencode"];
  if (tokens.includes("all")) {
    return validTargets;
  }

  const unique: SkillTarget[] = [];
  for (const token of tokens) {
    if (!isSkillTarget(token)) {
      throw new TsqError(
        "VALIDATION_ERROR",
        "skill targets must be comma-separated values of claude,codex,copilot,opencode,all",
        1,
      );
    }
    if (!unique.includes(token)) {
      unique.push(token);
    }
  }
  return unique;
}

function isSkillTarget(value: string): value is SkillTarget {
  return value === "claude" || value === "codex" || value === "copilot" || value === "opencode";
}

function parseDepDirection(raw?: string): DepDirection | undefined {
  if (!raw) return undefined;
  if (raw === "up" || raw === "down" || raw === "both") return raw;
  throw new TsqError("VALIDATION_ERROR", "direction must be up|down|both", 1);
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

function parseListFilter(options: ListCommandOptions): ListFilter {
  const filter: ListFilter = {};
  if (options.status) {
    filter.status = normalizeStatus(options.status);
  }
  if (options.assignee) {
    filter.assignee = options.assignee;
  }
  if (options.kind) {
    filter.kind = parseKind(options.kind);
  }
  if (options.label) {
    filter.label = options.label;
  }
  return filter;
}

function applyTreeDefaults(filter: ListFilter, options: ListCommandOptions): ListFilter {
  if (options.full || filter.status) {
    return filter;
  }
  return {
    ...filter,
    statuses: [...TREE_DEFAULT_STATUSES],
  };
}
