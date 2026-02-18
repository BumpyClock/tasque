# `tsq init` Setup Wizard Design Doc

## Overview
- Goals:
  - Reduce first-run friction for humans.
  - Keep setup fast and low-noise for power users.
  - Preserve deterministic non-interactive setup for agents/CI.
- Primary users:
  - New users running `tsq` for first time.
  - Agent workflows that need one-shot initialization.
  - Existing users updating skills/targets.
- Success criteria:
  - Human can complete init in <=30 seconds with default choices.
  - Agent can skip prompts entirely and configure in one command.
  - Zero ambiguity about what files/actions `init` will perform.

## Inputs and Constraints
- Platform targets: terminal CLI on macOS/Linux/Windows.
- Breakpoints: terminal width tiers (`>=120`, `90-119`, `<90`).
- Design system/component library: N/A (CLI/TUI text UI).
- Content requirements:
  - Explain minimal impact of each choice.
  - Show resulting plan before apply.
  - Keep wording concise and concrete.
- Technical constraints:
  - Must preserve existing `tsq init` behavior for script compatibility.
  - Must support full non-interactive path via flags.
  - Must avoid wizard in non-TTY contexts.

## Information Architecture
- Page hierarchy (CLI flow stages):
  - Entry gate (`auto wizard` vs `non-interactive`).
  - Setup choices (storage + optional skill install).
  - Confirmation summary.
  - Execution + result summary.
- Navigation model:
  - Linear, single-pass flow.
  - Explicit back/skip options per step.
- Key user flows:
  1. Human default: `tsq init` -> answer 3-5 prompts -> confirm -> done.
  2. Human explicit skip: `tsq init --no-wizard ...` -> immediate apply.
  3. Agent one-shot: `tsq init --no-wizard --yes --install-skill --skill-targets codex`.

## Design System Strategy
- Existing tokens/components to reuse:
  - Reuse current CLI render conventions (`created ...`, `skill ...`) and status wording.
  - Reuse existing validation/parser behavior for flags.
- Discovery notes:
  - Current init supports many flags but no guided prompt experience.
- New components needed:
  - `WizardStepHeader` (title + step counter).
  - `ChoicePrompt` (single-choice, default highlighted).
  - `PlanSummary` (resolved options before apply).
  - `NonInteractiveNotice` (explicit message when prompts are bypassed).
- Token naming conventions:
  - Keep textual markers simple: `[step x/y]`, `default`, `recommended`, `applied`.

## Layout and Responsive Behavior
- Wide terminal (`>=120`):
  - Two-column prompt rows: question left, defaults/help right.
- Medium (`90-119`):
  - Single-column with compact help line under each question.
- Narrow (`<90`):
  - One question per block; helper copy truncated; optional details behind `--verbose`.

## ASCII Layout
```text
Wide (>=120)
+--------------------------------------------------------------------------------+
| tsq init wizard                                           [step 2/4]           |
+--------------------------------------------------------------------------------+
| Install skill files now?                     (recommended: yes)               |
|  > yes - set up Claude/Codex/Copilot/OpenCode skill folders                  |
|    no  - initialize .tasque only                                               |
+--------------------------------------------------------------------------------+
| Planned changes:                                                               |
|  - create .tasque/config.json                                                  |
|  - create .tasque/events.jsonl                                                 |
|  - create .tasque/.gitignore                                                   |
|  - install skill "tasque" to: codex,claude                                    |
+--------------------------------------------------------------------------------+
| Enter=continue  b=back  s=skip wizard  q=quit                                  |
+--------------------------------------------------------------------------------+

Narrow (<90)
+----------------------------------------------+
| tsq init wizard [2/4]                        |
+----------------------------------------------+
| Install skill files now?                     |
| > yes (recommended)                          |
|   no                                         |
+----------------------------------------------+
| Planned: .tasque/* + skill install           |
| Enter continue | b back | s skip | q quit    |
+----------------------------------------------+
```

## Component Inventory
- `WizardStepHeader`
  - Purpose: keep orientation and progress.
  - States: normal, warning (if destructive/overwrite action).
- `ChoicePrompt`
  - Purpose: answer setup decisions quickly.
  - Variants: yes/no, single-select list, free-text (path overrides).
  - States: focused, default-selected, validation-error.
- `PlanSummary`
  - Purpose: transparency before writes.
  - States: ready, warning (conflict/overwrite).
- `ExecutionSummary`
  - Purpose: show concrete output file actions and next command hints.
  - States: success, partial success, failure.

## Interaction and State Matrix
- Primary actions:
  - Continue, Back, Skip wizard, Confirm apply, Cancel.
- Focus/active/disabled:
  - Active option indicated with `>` and explicit text, not color-only.
  - Disabled options include reason text.
- Loading/empty/error:
  - Loading: short `Applying setup...` with spinner-safe fallback text.
  - Empty: not applicable.
  - Error: print exact validation issue and return to offending step.
- Validation and inline feedback:
  - Invalid target list or paths are validated immediately.
  - Show accepted values in same prompt block.

## Visual System
- Color roles:
  - Monochrome-first; optional color accents only for success/warn/error labels.
- Typography scale:
  - Terminal native; rely on casing + spacing + prefix markers for hierarchy.
- Spacing and sizing:
  - 1-line separation between sections.
  - 2-line separation between major blocks.
- Iconography:
  - ASCII-safe markers only (`>`, `-`, `[ok]`, `[warn]`, `[error]`).

## Accessibility
- Keyboard navigation:
  - Full flow usable with keyboard only.
  - Enter confirms default; arrow keys or `j/k` move options.
- Focus order and states:
  - Strict top-to-bottom step progression; explicit focus marker.
- Contrast targets:
  - Must remain legible with no ANSI colors.
- Screen-reader/assistive notes:
  - Output should remain plain text and deterministic, no cursor-position dependence.

## Content Notes
- Copy tone and hierarchy:
  - Utility-first, short sentences, no marketing language.
- Empty-state copy:
  - For no-op mode: `No additional setup selected; initialized .tasque only.`
- Error messaging guidelines:
  - Lead with exact failing flag/value.
  - Follow with one-line fix example.

## Final CLI Contract
- Flag precedence:
  - `--no-wizard` wins and disables wizard unconditionally.
  - `--wizard` forces interactive mode, but only in TTY contexts.
  - `--preset` is valid only when wizard is enabled.
  - `--yes` auto-accepts wizard defaults/confirmation when wizard is enabled; otherwise no-op.
- Conflicting flags:
  - `--wizard` + `--no-wizard` is invalid.
  - `--preset` + `--no-wizard` is invalid.
- Existing init flags (`--install-skill`, `--uninstall-skill`, `--skill-targets`, `--skill-name`, `--force-skill-overwrite`, `--skill-dir-*`) remain authoritative in non-interactive mode and pre-seed wizard answers in interactive mode.

## Final Default Behavior
- TTY + no explicit disable:
  - Run wizard by default.
- Non-TTY:
  - Never run wizard automatically.
  - Reject `--wizard` and `--preset` with explicit non-interactive guidance.
- `--no-wizard`:
  - Always bypass wizard and execute directly from explicit flags/default init behavior.

## Minimal Wizard Step Set
- Step 1: baseline init confirmation (`.tasque/config.json`, `.tasque/events.jsonl`, `.tasque/.gitignore`).
- Step 2: skill action selection (`none`, `install`, `uninstall`).
- Step 3: skill details (targets, overwrite behavior, optional directory overrides) when step 2 is install/uninstall.
- Step 4: final resolved execution plan confirmation.

## Final Validation Rules
- `--install-skill` and `--uninstall-skill` are mutually exclusive.
- Skill-scoped flags without an active skill operation are validation errors.
- Invalid target values or malformed overrides fail before filesystem writes.
- Wizard should surface the same validation messages as non-interactive mode to keep behavior consistent.
