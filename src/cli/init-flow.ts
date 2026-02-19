import { createInterface } from "node:readline/promises";
import type { InitInput } from "../app/service";
import { TsqError } from "../errors";
import type { SkillTarget } from "../skills/types";
import {
  type InitCommandOptions,
  type InitPreset,
  asOptionalString,
  parseInitPreset,
  parseSkillTargets,
} from "./parsers";

const ALL_SKILL_TARGETS: SkillTarget[] = ["claude", "codex", "copilot", "opencode"];

type SkillAction = "none" | "install" | "uninstall";

export interface InitResolutionContext {
  rawArgs: string[];
  isTTY: boolean;
  json: boolean;
}

export interface WizardSeed {
  action: SkillAction;
  skillTargets: SkillTarget[];
  skillName: string;
  forceSkillOverwrite: boolean;
  skillDirClaude?: string;
  skillDirCodex?: string;
  skillDirCopilot?: string;
  skillDirOpencode?: string;
}

export type InitPlan =
  | { mode: "non_interactive"; input: InitInput }
  | { mode: "wizard"; autoAccept: boolean; seed: WizardSeed };

interface InteractiveIo {
  stdin: NodeJS.ReadStream;
  stdout: NodeJS.WriteStream;
}

export function resolveInitPlan(
  options: InitCommandOptions,
  context: InitResolutionContext,
): InitPlan {
  const hasWizard = hasFlag(context.rawArgs, "--wizard");
  const hasNoWizard = hasFlag(context.rawArgs, "--no-wizard");
  const preset = options.preset ? parseInitPreset(options.preset) : undefined;

  if (hasWizard && hasNoWizard) {
    throw new TsqError("VALIDATION_ERROR", "cannot combine --wizard with --no-wizard", 1);
  }

  if (preset && hasNoWizard) {
    throw new TsqError("VALIDATION_ERROR", "cannot combine --preset with --no-wizard", 1);
  }

  if (options.installSkill && options.uninstallSkill) {
    throw new TsqError(
      "VALIDATION_ERROR",
      "cannot combine --install-skill with --uninstall-skill",
      1,
    );
  }

  if (!context.isTTY && hasWizard) {
    throw new TsqError("VALIDATION_ERROR", "--wizard requires an interactive TTY", 1);
  }

  if (!context.isTTY && preset) {
    throw new TsqError("VALIDATION_ERROR", "--preset requires an interactive TTY", 1);
  }

  if (context.json && hasWizard) {
    throw new TsqError("VALIDATION_ERROR", "--wizard is not supported with --json", 1);
  }

  if (context.json && preset) {
    throw new TsqError("VALIDATION_ERROR", "--preset is not supported with --json", 1);
  }

  const hasExplicitSkillAction = Boolean(options.installSkill || options.uninstallSkill);
  const wizardEnabled =
    !hasNoWizard && (hasWizard || (!hasExplicitSkillAction && context.isTTY && !context.json));
  if (!wizardEnabled) {
    return {
      mode: "non_interactive",
      input: resolveNonInteractiveInput(options),
    };
  }

  return {
    mode: "wizard",
    autoAccept: Boolean(options.yes),
    seed: resolveWizardSeed(options, preset),
  };
}

export async function runInitWizard(seed: WizardSeed, io: InteractiveIo, autoAccept: boolean) {
  const rl = createInterface({
    input: io.stdin,
    output: io.stdout,
    terminal: true,
  });

  const state: WizardSeed = {
    ...seed,
    skillTargets: [...seed.skillTargets],
  };

  try {
    if (autoAccept) {
      printHeader(io.stdout, 4, 4);
      printPlanSummary(io.stdout, state);
      io.stdout.write("\n--yes enabled: applying defaults and confirmation automatically.\n");
      return buildInitInputFromSeed(state);
    }

    let step = 1;
    while (true) {
      if (step === 1) {
        printHeader(io.stdout, 1, 4);
        io.stdout.write(
          "This will initialize: .tasque/config.json, .tasque/events.jsonl, .tasque/.gitignore\n",
        );
        const decision = await askToken(rl, "Continue setup? [Y/n/s/q] ");
        if (isYes(decision)) {
          step = 2;
          continue;
        }
        if (decision === "s") {
          step = 4;
          continue;
        }
        if (isNo(decision) || decision === "q") {
          throw new TsqError("VALIDATION_ERROR", "init canceled by user", 1);
        }
        printInvalid(io.stdout);
        continue;
      }

      if (step === 2) {
        printHeader(io.stdout, 2, 4);
        io.stdout.write("Select skill action:\n");
        io.stdout.write("  1) install\n");
        io.stdout.write("  2) uninstall\n");
        io.stdout.write("  3) none\n");
        const defaultChoice = defaultActionChoice(state.action);
        const answer = await askToken(
          rl,
          `Skill action [1/2/3] (default ${defaultChoice}, b=back, s=skip, q=quit) `,
        );

        if (answer === "b") {
          step = 1;
          continue;
        }
        if (answer === "s") {
          step = 4;
          continue;
        }
        if (answer === "q") {
          throw new TsqError("VALIDATION_ERROR", "init canceled by user", 1);
        }

        const normalized = answer.length === 0 ? defaultChoice : answer;
        if (normalized === "1" || normalized === "install") {
          state.action = "install";
          step = 3;
          continue;
        }
        if (normalized === "2" || normalized === "uninstall") {
          state.action = "uninstall";
          step = 3;
          continue;
        }
        if (normalized === "3" || normalized === "none") {
          state.action = "none";
          step = 4;
          continue;
        }

        printInvalid(io.stdout);
        continue;
      }

      if (step === 3) {
        if (state.action === "none") {
          step = 4;
          continue;
        }

        printHeader(io.stdout, 3, 4);
        const defaultTargets = formatTargets(state.skillTargets);
        const targetsAnswer = await askToken(
          rl,
          `Skill targets [all or csv] (default ${defaultTargets}, b=back, s=skip, q=quit) `,
        );

        if (targetsAnswer === "b") {
          step = 2;
          continue;
        }
        if (targetsAnswer === "s") {
          step = 4;
          continue;
        }
        if (targetsAnswer === "q") {
          throw new TsqError("VALIDATION_ERROR", "init canceled by user", 1);
        }

        try {
          state.skillTargets =
            targetsAnswer.length === 0 ? [...state.skillTargets] : parseSkillTargets(targetsAnswer);
        } catch (error) {
          if (error instanceof TsqError) {
            io.stdout.write(`${error.code}: ${error.message}\n`);
            continue;
          }
          throw error;
        }

        const nameAnswer = await askToken(
          rl,
          `Skill name (default ${state.skillName}, b=back, s=skip, q=quit) `,
        );
        if (nameAnswer === "b") {
          step = 2;
          continue;
        }
        if (nameAnswer === "s") {
          step = 4;
          continue;
        }
        if (nameAnswer === "q") {
          throw new TsqError("VALIDATION_ERROR", "init canceled by user", 1);
        }

        state.skillName = nameAnswer.length === 0 ? state.skillName : nameAnswer;

        if (state.action === "install") {
          const forceAnswer = await askToken(
            rl,
            `Force overwrite unmanaged skill dirs? [y/N] (default ${state.forceSkillOverwrite ? "y" : "n"}) `,
          );
          if (forceAnswer === "q") {
            throw new TsqError("VALIDATION_ERROR", "init canceled by user", 1);
          }
          if (forceAnswer === "b") {
            step = 2;
            continue;
          }
          if (forceAnswer === "s") {
            step = 4;
            continue;
          }

          if (isYes(forceAnswer)) {
            state.forceSkillOverwrite = true;
          } else if (isNo(forceAnswer)) {
            state.forceSkillOverwrite = false;
          }
        }

        step = 4;
        continue;
      }

      printHeader(io.stdout, 4, 4);
      printPlanSummary(io.stdout, state);
      const confirm = await askToken(rl, "Apply this setup? [Y/n/b/s/q] ");

      if (isYes(confirm) || confirm === "s") {
        return buildInitInputFromSeed(state);
      }
      if (confirm === "b") {
        step = state.action === "none" ? 2 : 3;
        continue;
      }
      if (isNo(confirm) || confirm === "q") {
        throw new TsqError("VALIDATION_ERROR", "init canceled by user", 1);
      }

      printInvalid(io.stdout);
    }
  } finally {
    rl.close();
  }
}

function resolveNonInteractiveInput(options: InitCommandOptions): InitInput {
  const hasSkillOperation = Boolean(options.installSkill || options.uninstallSkill);

  if (!hasSkillOperation && hasSkillScopedFlags(options)) {
    throw new TsqError(
      "VALIDATION_ERROR",
      "skill options require --install-skill or --uninstall-skill",
      1,
    );
  }

  return {
    installSkill: Boolean(options.installSkill),
    uninstallSkill: Boolean(options.uninstallSkill),
    skillTargets: hasSkillOperation ? parseSkillTargets(options.skillTargets ?? "all") : undefined,
    skillName: hasSkillOperation ? (asOptionalString(options.skillName) ?? "tasque") : undefined,
    forceSkillOverwrite: Boolean(options.forceSkillOverwrite),
    skillDirClaude: asOptionalString(options.skillDirClaude),
    skillDirCodex: asOptionalString(options.skillDirCodex),
    skillDirCopilot: asOptionalString(options.skillDirCopilot),
    skillDirOpencode: asOptionalString(options.skillDirOpencode),
  };
}

function resolveWizardSeed(
  options: InitCommandOptions,
  preset: InitPreset | undefined,
): WizardSeed {
  const defaults = resolvePresetDefaults(preset);
  const explicitAction: SkillAction | undefined = options.installSkill
    ? "install"
    : options.uninstallSkill
      ? "uninstall"
      : undefined;

  const action = explicitAction ?? defaults.action;
  const skillTargets =
    options.skillTargets !== undefined
      ? parseSkillTargets(options.skillTargets)
      : [...defaults.skillTargets];
  const skillName = asOptionalString(options.skillName) ?? defaults.skillName;
  const forceSkillOverwrite = Boolean(options.forceSkillOverwrite || defaults.forceSkillOverwrite);

  if (action === "none" && hasSkillScopedFlags(options)) {
    throw new TsqError(
      "VALIDATION_ERROR",
      "skill options require --install-skill or --uninstall-skill (or preset that enables skill action)",
      1,
    );
  }

  return {
    action,
    skillTargets,
    skillName,
    forceSkillOverwrite,
    skillDirClaude: asOptionalString(options.skillDirClaude),
    skillDirCodex: asOptionalString(options.skillDirCodex),
    skillDirCopilot: asOptionalString(options.skillDirCopilot),
    skillDirOpencode: asOptionalString(options.skillDirOpencode),
  };
}

function resolvePresetDefaults(preset: InitPreset | undefined): WizardSeed {
  if (preset === "standard") {
    return {
      action: "install",
      skillTargets: [...ALL_SKILL_TARGETS],
      skillName: "tasque",
      forceSkillOverwrite: false,
    };
  }

  if (preset === "full") {
    return {
      action: "install",
      skillTargets: [...ALL_SKILL_TARGETS],
      skillName: "tasque",
      forceSkillOverwrite: true,
    };
  }

  return {
    action: "none",
    skillTargets: [...ALL_SKILL_TARGETS],
    skillName: "tasque",
    forceSkillOverwrite: false,
  };
}

function buildInitInputFromSeed(seed: WizardSeed): InitInput {
  const skillEnabled = seed.action !== "none";
  return {
    installSkill: seed.action === "install",
    uninstallSkill: seed.action === "uninstall",
    skillTargets: skillEnabled ? [...seed.skillTargets] : undefined,
    skillName: skillEnabled ? seed.skillName : undefined,
    forceSkillOverwrite: seed.action === "install" ? seed.forceSkillOverwrite : false,
    skillDirClaude: seed.skillDirClaude,
    skillDirCodex: seed.skillDirCodex,
    skillDirCopilot: seed.skillDirCopilot,
    skillDirOpencode: seed.skillDirOpencode,
  };
}

function hasSkillScopedFlags(options: InitCommandOptions): boolean {
  return Boolean(
    options.skillTargets !== undefined ||
      options.skillName !== undefined ||
      options.forceSkillOverwrite ||
      asOptionalString(options.skillDirClaude) ||
      asOptionalString(options.skillDirCodex) ||
      asOptionalString(options.skillDirCopilot) ||
      asOptionalString(options.skillDirOpencode),
  );
}

function hasFlag(rawArgs: string[], flag: string): boolean {
  return rawArgs.includes(flag);
}

function printHeader(stdout: NodeJS.WriteStream, step: number, total: number): void {
  stdout.write(`\ntsq init wizard [step ${step}/${total}]\n`);
}

function printPlanSummary(stdout: NodeJS.WriteStream, seed: WizardSeed): void {
  stdout.write("Planned changes:\n");
  stdout.write("- create .tasque/config.json\n");
  stdout.write("- create .tasque/events.jsonl\n");
  stdout.write("- create .tasque/.gitignore\n");
  if (seed.action === "install") {
    stdout.write(
      `- install skill \"${seed.skillName}\" to targets: ${formatTargets(seed.skillTargets)}${
        seed.forceSkillOverwrite ? " (force overwrite)" : ""
      }\n`,
    );
  } else if (seed.action === "uninstall") {
    stdout.write(
      `- uninstall skill \"${seed.skillName}\" from targets: ${formatTargets(seed.skillTargets)}\n`,
    );
  } else {
    stdout.write("- no skill operation\n");
  }
}

async function askToken(rl: ReturnType<typeof createInterface>, question: string): Promise<string> {
  const answer = await rl.question(`${question.trim()} `);
  return answer.trim().toLowerCase();
}

function defaultActionChoice(action: SkillAction): "1" | "2" | "3" {
  if (action === "install") return "1";
  if (action === "uninstall") return "2";
  return "3";
}

function formatTargets(targets: SkillTarget[]): string {
  if (targets.length === ALL_SKILL_TARGETS.length) {
    const normalized = [...targets].sort();
    const allNormalized = [...ALL_SKILL_TARGETS].sort();
    const isAll = normalized.every((value, index) => value === allNormalized[index]);
    if (isAll) {
      return "all";
    }
  }
  return targets.join(",");
}

function isYes(value: string): boolean {
  return value.length === 0 || value === "y" || value === "yes";
}

function isNo(value: string): boolean {
  return value === "n" || value === "no";
}

function printInvalid(stdout: NodeJS.WriteStream): void {
  stdout.write("Invalid input. Use the shown options.\n");
}
