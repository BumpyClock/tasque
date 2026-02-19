import { describe, expect, it } from "bun:test";
import { resolveInitPlan } from "../src/cli/init-flow";
import type { InitCommandOptions } from "../src/cli/parsers";
import { TsqError } from "../src/errors";

function expectValidationError(fn: () => unknown, contains: string): void {
  expect(fn).toThrow(TsqError);
  try {
    fn();
  } catch (error) {
    expect(error).toBeInstanceOf(TsqError);
    const tsqError = error as TsqError;
    expect(tsqError.code).toBe("VALIDATION_ERROR");
    expect(tsqError.message).toContain(contains);
  }
}

function resolve(
  options: InitCommandOptions,
  rawArgs: string[],
  overrides?: { isTTY?: boolean; json?: boolean },
) {
  return resolveInitPlan(options, {
    rawArgs,
    isTTY: overrides?.isTTY ?? false,
    json: overrides?.json ?? false,
  });
}

describe("init flow resolution", () => {
  it("rejects --wizard in non-tty mode", () => {
    expectValidationError(
      () => resolve({ wizard: true }, ["init", "--wizard"], { isTTY: false }),
      "interactive TTY",
    );
  });

  it("rejects --preset in non-tty mode", () => {
    expectValidationError(
      () => resolve({ preset: "minimal" }, ["init", "--preset", "minimal"], { isTTY: false }),
      "interactive TTY",
    );
  });

  it("rejects --wizard with --no-wizard", () => {
    expectValidationError(
      () => resolve({ wizard: true }, ["init", "--wizard", "--no-wizard"], { isTTY: true }),
      "cannot combine --wizard with --no-wizard",
    );
  });

  it("rejects --preset with --no-wizard", () => {
    expectValidationError(
      () =>
        resolve({ preset: "standard" }, ["init", "--no-wizard", "--preset", "standard"], {
          isTTY: true,
        }),
      "cannot combine --preset with --no-wizard",
    );
  });

  it("defaults to wizard in tty mode", () => {
    const plan = resolve({}, ["init"], { isTTY: true });
    expect(plan.mode).toBe("wizard");
  });

  it("disables wizard in tty mode when --json is enabled", () => {
    const plan = resolve({}, ["init"], { isTTY: true, json: true });
    expect(plan.mode).toBe("non_interactive");
  });

  it("disables auto wizard in tty mode when explicit skill action is provided", () => {
    const plan = resolve({ installSkill: true }, ["init", "--install-skill"], { isTTY: true });
    expect(plan.mode).toBe("non_interactive");
    if (plan.mode !== "non_interactive") {
      throw new Error("expected non_interactive mode");
    }
    expect(plan.input.installSkill).toBe(true);
    expect(plan.input.uninstallSkill).toBe(false);
  });

  it("marks wizard plan as auto-accept when --yes is provided", () => {
    const plan = resolve({ yes: true }, ["init"], { isTTY: true });
    expect(plan.mode).toBe("wizard");
    if (plan.mode !== "wizard") {
      throw new Error("expected wizard mode");
    }
    expect(plan.autoAccept).toBe(true);
  });

  it("maps standard preset to install defaults", () => {
    const plan = resolve({ preset: "standard" }, ["init", "--preset", "standard"], { isTTY: true });
    expect(plan.mode).toBe("wizard");
    if (plan.mode !== "wizard") {
      throw new Error("expected wizard mode");
    }
    expect(plan.seed.action).toBe("install");
    expect(plan.seed.skillName).toBe("tasque");
    expect(plan.seed.forceSkillOverwrite).toBe(false);
  });

  it("maps full preset to install with force overwrite", () => {
    const plan = resolve({ preset: "full" }, ["init", "--preset", "full"], { isTTY: true });
    expect(plan.mode).toBe("wizard");
    if (plan.mode !== "wizard") {
      throw new Error("expected wizard mode");
    }
    expect(plan.seed.action).toBe("install");
    expect(plan.seed.forceSkillOverwrite).toBe(true);
  });

  it("uses explicit skill action over preset", () => {
    const plan = resolve(
      {
        preset: "full",
        uninstallSkill: true,
      },
      ["init", "--preset", "full", "--uninstall-skill"],
      { isTTY: true },
    );
    expect(plan.mode).toBe("non_interactive");
    if (plan.mode !== "non_interactive") {
      throw new Error("expected non_interactive mode");
    }
    expect(plan.input.uninstallSkill).toBe(true);
    expect(plan.input.installSkill).toBe(false);
  });

  it("uses explicit skill targets over preset defaults", () => {
    const plan = resolve(
      {
        preset: "standard",
        skillTargets: "codex",
      },
      ["init", "--preset", "standard", "--skill-targets", "codex"],
      { isTTY: true },
    );
    expect(plan.mode).toBe("wizard");
    if (plan.mode !== "wizard") {
      throw new Error("expected wizard mode");
    }
    expect(plan.seed.skillTargets).toEqual(["codex"]);
  });

  it("rejects skill-scoped flags without skill action in non-interactive mode", () => {
    expectValidationError(
      () =>
        resolve({ skillTargets: "codex" }, ["init", "--no-wizard", "--skill-targets", "codex"], {
          isTTY: false,
        }),
      "skill options require --install-skill or --uninstall-skill",
    );
  });

  it("resolves non-interactive install options when wizard is disabled", () => {
    const plan = resolve(
      {
        installSkill: true,
        skillTargets: "codex",
        skillName: "custom",
        forceSkillOverwrite: true,
      },
      ["init", "--no-wizard", "--install-skill", "--skill-targets", "codex"],
      { isTTY: false },
    );

    expect(plan.mode).toBe("non_interactive");
    if (plan.mode !== "non_interactive") {
      throw new Error("expected non_interactive mode");
    }

    expect(plan.input.installSkill).toBe(true);
    expect(plan.input.uninstallSkill).toBe(false);
    expect(plan.input.skillTargets).toEqual(["codex"]);
    expect(plan.input.skillName).toBe("custom");
    expect(plan.input.forceSkillOverwrite).toBe(true);
  });
});
