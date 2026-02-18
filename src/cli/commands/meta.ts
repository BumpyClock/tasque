import type { Command } from "commander";
import { normalizeStatus } from "../../app/runtime";
import type { RunAction, RuntimeDeps } from "../action";
import {
  type GlobalOpts,
  type InitCommandOptions,
  type WatchCommandOptions,
  asOptionalString,
  parsePositiveInt,
  parseSkillTargets,
} from "../parsers";
import { printHistory, printOrphansResult, printRepairResult } from "../render";
import { startWatch } from "../watch";

export function registerMetaCommands(
  program: Command,
  deps: RuntimeDeps,
  runAction: RunAction,
): void {
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
    .command("orphans")
    .description("List orphaned deps and links (read-only)")
    .action(async function action() {
      await runAction(this, async () => deps.service.orphans(), {
        jsonData: (data) => data,
        human: (data) => printOrphansResult(data),
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
            limit: options.limit ? parsePositiveInt(options.limit, "limit", 1, 10000) : undefined,
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

  program
    .command("watch")
    .option("--interval <seconds>", "refresh interval in seconds (1-60)", "2")
    .option(
      "--status <status>",
      "filter by status (repeatable, comma-separated)",
      "open,in_progress",
    )
    .option("--assignee <assignee>", "filter by assignee")
    .option("--tree", "show parent-child hierarchy")
    .option("--once", "single frame render, then exit")
    .description("Live view of active tasks")
    .action(async function action(options: WatchCommandOptions) {
      const globalOpts = this.optsWithGlobals<GlobalOpts>();
      const statuses = options.status.split(",").map((s: string) => normalizeStatus(s.trim()));
      await startWatch(deps.service, {
        interval: parsePositiveInt(options.interval, "interval", 1, 60),
        statuses,
        assignee: asOptionalString(options.assignee),
        tree: Boolean(options.tree),
        once: Boolean(options.once),
        json: Boolean(globalOpts.json),
      });
    });
}
