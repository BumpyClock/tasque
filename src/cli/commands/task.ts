import type { Command } from "commander";
import { normalizeStatus, parsePriority } from "../../app/runtime";
import { readStdinContent } from "../../app/stdin";
import { TsqError } from "../../errors";
import type { RunAction, RuntimeDeps } from "../action";
import {
  type CreateCommandOptions,
  type ListCommandOptions,
  type StaleCommandOptions,
  type UpdateCommandOptions,
  applyTreeDefaults,
  asOptionalString,
  collectCsvOption,
  parseKind,
  parseLane,
  parseListFilter,
  parseNonNegativeInt,
  parsePlanningState,
  parsePositiveInt,
  validateExplicitId,
} from "../parsers";
import { printMergeResult, printTask, printTaskList, printTaskTree } from "../render";

export function registerTaskCommands(
  program: Command,
  deps: RuntimeDeps,
  runAction: RunAction,
): void {
  program
    .command("create")
    .argument("<title>", "task title")
    .option("--kind <kind>", "task kind: task|feature|epic", "task")
    .option("-p, --priority <priority>", "priority 0..3", "2")
    .option("--parent <id>", "parent task ID")
    .option("--description <text>", "task description")
    .option("--external-ref <ref>", "external reference (ticket/URL/id)")
    .option("--planning <state>", "planning state: needs_planning|planned")
    .option("--needs-planning", "shorthand for --planning needs_planning")
    .option("--id <id>", "explicit task ID (tsq-<8 crockford base32>)")
    .option("--body-file <path>", "read description from file (use - for stdin)")
    .description("Create task")
    .action(async function action(title: string, options: CreateCommandOptions) {
      await runAction(
        this,
        async (opts) => {
          const kind = parseKind(options.kind ?? "task");
          const priority = parsePriority(options.priority ?? "2");
          if (options.planning && options.needsPlanning) {
            throw new TsqError(
              "VALIDATION_ERROR",
              "cannot combine --planning with --needs-planning",
              1,
            );
          }
          if (options.id && options.parent) {
            throw new TsqError("VALIDATION_ERROR", "cannot combine --id with --parent", 1);
          }
          if (asOptionalString(options.description) !== undefined && options.bodyFile) {
            throw new TsqError(
              "VALIDATION_ERROR",
              "cannot combine --description with --body-file",
              1,
            );
          }
          const planning_state = options.needsPlanning
            ? ("needs_planning" as const)
            : options.planning
              ? parsePlanningState(options.planning)
              : undefined;

          let bodyFile: string | undefined;
          if (options.bodyFile) {
            if (options.bodyFile === "-") {
              bodyFile = await readStdinContent();
            } else {
              const { readFile } = await import("node:fs/promises");
              bodyFile = await readFile(options.bodyFile, "utf8");
            }
            if (bodyFile.trim().length === 0) {
              throw new TsqError("VALIDATION_ERROR", "body file content must not be empty", 1);
            }
          }

          return deps.service.create({
            title,
            kind,
            priority,
            parent: options.parent,
            description: asOptionalString(options.description),
            externalRef: asOptionalString(options.externalRef),
            exactId: opts.exactId,
            planning_state,
            explicitId: options.id ? validateExplicitId(options.id) : undefined,
            bodyFile,
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

  const listFlagState = {
    assignee: false,
    unassigned: false,
  };

  program
    .command("list")
    .option("--status <status>", "filter by status")
    .option("--assignee <assignee>", "filter by assignee")
    .option("--external-ref <ref>", "filter by external reference")
    .option("--kind <kind>", "filter by kind")
    .option("--label <label>", "filter by label")
    .option(
      "--label-any <label>",
      "filter by any label (repeatable, comma-separated)",
      collectCsvOption("label-any"),
      [],
    )
    .option("--created-after <iso>", "filter by created_at after timestamp")
    .option("--updated-after <iso>", "filter by updated_at after timestamp")
    .option("--closed-after <iso>", "filter by closed_at after timestamp")
    .option("--unassigned", "filter tasks without assignee")
    .option(
      "--id <id>",
      "filter by task id (repeatable, comma-separated)",
      collectCsvOption("id"),
      [],
    )
    .option("--tree", "render parent/child hierarchy")
    .option("--full", "with --tree, include closed/blocked/canceled items")
    .option("--planning <state>", "filter by planning state: needs_planning|planned")
    .description("List tasks")
    .on("option:assignee", () => {
      listFlagState.assignee = true;
    })
    .on("option:unassigned", () => {
      listFlagState.unassigned = true;
    })
    .action(async function action(options: ListCommandOptions) {
      try {
        const filter = parseListFilter(options, listFlagState.unassigned, listFlagState.assignee);
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
      } finally {
        listFlagState.assignee = false;
        listFlagState.unassigned = false;
      }
    });

  program
    .command("stale")
    .option("--days <n>", "stale threshold in days", "30")
    .option("--status <status>", "single status scope")
    .option("--assignee <assignee>", "filter by assignee")
    .option("--limit <n>", "max stale tasks to return")
    .description("List stale tasks")
    .action(async function action(options: StaleCommandOptions) {
      await runAction(
        this,
        async () =>
          deps.service.stale({
            days: parseNonNegativeInt(options.days ?? "30", "days"),
            status: options.status ? normalizeStatus(options.status) : undefined,
            assignee: asOptionalString(options.assignee),
            limit: options.limit ? parsePositiveInt(options.limit, "limit", 1, 10000) : undefined,
          }),
        {
          jsonData: (data) => data,
          human: (data) => printTaskList(data.tasks),
        },
      );
    });

  program
    .command("ready")
    .option("--lane <lane>", "filter by lane: planning|coding")
    .description("List ready tasks")
    .action(async function action(options: { lane?: string }) {
      await runAction(
        this,
        async () => {
          const lane = options.lane ? parseLane(options.lane) : undefined;
          return deps.service.ready(lane);
        },
        {
          jsonData: (tasks) => ({ tasks }),
          human: (tasks) => printTaskList(tasks),
        },
      );
    });

  program
    .command("update")
    .argument("<id>", "task id")
    .option("--title <title>", "new title")
    .option("--description <text>", "set task description")
    .option("--clear-description", "clear task description")
    .option("--external-ref <ref>", "set external reference")
    .option("--clear-external-ref", "clear external reference")
    .option("--status <status>", "status value")
    .option("--priority <priority>", "priority 0..3")
    .option("--claim", "claim this task")
    .option("--assignee <assignee>", "assignee for claim")
    .option("--require-spec", "with --claim, require attached spec to pass validation")
    .option("--planning <state>", "set planning state: needs_planning|planned")
    .description("Update task")
    .action(async function action(id: string, options: UpdateCommandOptions) {
      await runAction(
        this,
        async (opts) => {
          const claim = Boolean(options.claim);
          const requireSpec = Boolean(options.requireSpec);
          const hasDescription = asOptionalString(options.description) !== undefined;
          const clearDescription = Boolean(options.clearDescription);
          const hasExternalRef = asOptionalString(options.externalRef) !== undefined;
          const clearExternalRef = Boolean(options.clearExternalRef);
          if (hasDescription && clearDescription) {
            throw new TsqError(
              "VALIDATION_ERROR",
              "cannot combine --description with --clear-description",
              1,
            );
          }
          if (hasExternalRef && clearExternalRef) {
            throw new TsqError(
              "VALIDATION_ERROR",
              "cannot combine --external-ref with --clear-external-ref",
              1,
            );
          }
          if (!claim && requireSpec) {
            throw new TsqError("VALIDATION_ERROR", "--require-spec requires --claim", 1);
          }
          if (claim) {
            if (
              options.title ||
              options.status ||
              options.priority ||
              hasDescription ||
              clearDescription ||
              hasExternalRef ||
              clearExternalRef
            ) {
              throw new TsqError(
                "VALIDATION_ERROR",
                "cannot combine --claim with --title/--description/--clear-description/--external-ref/--clear-external-ref/--status/--priority",
                1,
              );
            }
            return deps.service.claim({
              id,
              assignee: asOptionalString(options.assignee),
              requireSpec,
              exactId: opts.exactId,
            });
          }
          return deps.service.update({
            id,
            title: asOptionalString(options.title),
            description: asOptionalString(options.description),
            clearDescription,
            externalRef: asOptionalString(options.externalRef),
            clearExternalRef,
            status: options.status ? normalizeStatus(String(options.status)) : undefined,
            priority: options.priority ? parsePriority(String(options.priority)) : undefined,
            planning_state: options.planning ? parsePlanningState(options.planning) : undefined,
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
    .command("duplicate")
    .argument("<id>", "duplicate task id")
    .requiredOption("--of <canonical-id>", "canonical task id")
    .option("--reason <text>", "duplicate reason")
    .description("Mark a task as duplicate of canonical task")
    .action(async function action(id: string, options: { of: string; reason?: string }) {
      await runAction(
        this,
        async (opts) =>
          deps.service.duplicate({
            source: id,
            canonical: options.of,
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
    .command("duplicates")
    .option("--limit <n>", "max duplicate groups", "20")
    .description("Dry-run duplicate detector scaffold")
    .action(async function action(options: { limit?: string }) {
      await runAction(
        this,
        async () =>
          deps.service.duplicateCandidates(
            parsePositiveInt(options.limit ?? "20", "limit", 1, 200),
          ),
        {
          jsonData: (data) => data,
          human: (data) => {
            if (data.groups.length === 0) {
              console.log("no duplicate candidates");
              return;
            }
            console.log(`scanned=${data.scanned} groups=${data.groups.length}`);
            for (const group of data.groups) {
              const ids = group.tasks.map((task) => task.id).join(",");
              console.log(`${group.key}: ${ids}`);
            }
          },
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
    .command("merge")
    .argument("<sources...>", "source task IDs to merge")
    .requiredOption("--into <target>", "target task ID to merge into")
    .option("--reason <text>", "merge reason")
    .option("--force", "allow merging into closed/canceled target")
    .option("--dry-run", "preview merge without applying")
    .description("Merge tasks as duplicates into a target")
    .action(async function action(
      sources: string[],
      options: {
        into: string;
        reason?: string;
        force?: boolean;
        dryRun?: boolean;
      },
    ) {
      await runAction(
        this,
        async (opts) =>
          deps.service.merge({
            sources,
            into: options.into,
            reason: options.reason,
            force: Boolean(options.force),
            dryRun: Boolean(options.dryRun),
            exactId: opts.exactId,
          }),
        {
          jsonData: (data) => data,
          human: (data) => printMergeResult(data),
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
          jsonData: (tasks) => ({ tasks }),
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
        jsonData: (tasks) => ({ tasks }),
        human: (tasks) => {
          for (const task of tasks) {
            printTask(task);
          }
        },
      });
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
}
