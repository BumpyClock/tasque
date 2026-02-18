# `tsq init` Wizard UX Design Doc (Refined)

Read when: implementing `tsq init` wizard flow, `--wizard/--no-wizard/--preset/--yes` behavior, or init UX tests.

## Overview
- Goals:
  - Reduce first-run friction for humans.
  - Preserve deterministic non-interactive setup for agents/CI.
  - Make write actions explicit before execution.
- Primary users:
  - Human developers running `tsq init` for first use.
  - Agent/automation workflows needing one-shot setup.
- Success criteria:
  - Human completes default wizard in <= 30 seconds.
  - Non-interactive flows remain stable and scriptable.
  - Validation failures are clear and actionable on first read.

## Inputs and Constraints
- Platform targets: terminal CLI (macOS/Linux/Windows).
- Breakpoints:
  - Wide: `>= 120` columns.
  - Medium: `90-119` columns.
  - Narrow: `< 90` columns.
- Design system/component library: terminal text UI only.
- Content requirements:
  - Utility-first copy.
  - Minimal prompt count.
  - Explicit plan summary before writes.
- Technical constraints:
  - Existing `tsq init` scripted behavior must stay backward compatible.
  - Wizard must never auto-run in non-TTY.
  - Existing init flags remain authoritative when provided.

## Information Architecture
- Flow hierarchy:
  1. Mode resolution (interactive vs non-interactive).
  2. Optional wizard steps (4-step max).
  3. Resolved execution plan summary.
  4. Apply and print result summary.
- Navigation model:
  - Linear single-pass flow.
  - Explicit `Back`, `Skip wizard`, and `Quit` at each wizard step.
- Key user flows:
  1. Human default: `tsq init` in TTY -> guided prompts -> confirm -> apply.
  2. Human explicit non-interactive: `tsq init --no-wizard ...` -> direct apply.
  3. Agent one-shot: `tsq init --no-wizard --install-skill --skill-targets codex --yes`.

## Design Direction
- Personality: Utility & Function with Precision & Density.
- Why:
  - Fits developer CLI expectations.
  - Keeps cognitive load low.
  - Prioritizes deterministic behavior over decorative UI.
- Rules:
  - Monochrome-first output; color only for success/warn/error labels.
  - Short lines and consistent markers (`[step]`, `[ok]`, `[warn]`, `[error]`).
  - No cursor-position tricks or full-screen modes.

## Design System Strategy
- Reuse:
  - Existing terminal render conventions used across `tsq`.
  - Existing validation language and error envelope conventions.
- New CLI “components”:
  - `WizardStepHeader`: `tsq init wizard [step x/4]`.
  - `ChoicePrompt`: single-select or yes/no with default marker.
  - `PlanSummary`: resolved action list before apply.
  - `ExecutionSummary`: final actions and next-step hints.
- Token conventions:
  - Step marker: `[step x/4]`.
  - Defaults: `(default)`.
  - Recommendations: `(recommended)`.
  - Status labels: `[ok]`, `[warn]`, `[error]`.

## Layout and Responsive Behavior
- Desktop/wide (`>=120`):
  - Two-column prompt layout.
  - Right-side helper text for defaults/impacts.
- Tablet/medium (`90-119`):
  - Single-column prompt + one helper line.
- Mobile/narrow (`<90`):
  - Single-column blocks.
  - Keep one decision per block.
  - Truncate helper copy to one line; defer details to summary.

## ASCII Layout
```text
Wide (>=120)
+--------------------------------------------------------------------------------+
| tsq init wizard                                               [step 3/4]       |
+--------------------------------------------------------------------------------+
| Skill action:                      (default: install)                          |
|  > install - add managed skill files for selected targets                      |
|    uninstall - remove managed skill files                                      |
|    none - initialize .tasque only                                              |
+--------------------------------------------------------------------------------+
| Planned changes:                                                               |
|  - create .tasque/config.json                                                  |
|  - create .tasque/events.jsonl                                                 |
|  - create .tasque/.gitignore                                                   |
|  - install skill "tasque" to targets: all                                     |
+--------------------------------------------------------------------------------+
| Enter continue   b back   s skip wizard   q quit                               |
+--------------------------------------------------------------------------------+

Narrow (<90)
+----------------------------------------------+
| tsq init wizard [3/4]                        |
+----------------------------------------------+
| Skill action                                 |
| > install (default)                          |
|   uninstall                                  |
|   none                                       |
+----------------------------------------------+
| Planned: .tasque/* + skill install           |
| Enter continue | b back | s skip | q quit    |
+----------------------------------------------+
```

## Component Inventory
- `WizardStepHeader`
  - Purpose: orientation and progress.
  - States: normal, warning.
- `ChoicePrompt`
  - Purpose: quick decision capture.
  - Variants: yes/no, single-select, free-text (override paths).
  - States: focused, default-selected, invalid.
- `PlanSummary`
  - Purpose: transparency before writes.
  - States: ready, warning (overwrite/force).
- `ExecutionSummary`
  - Purpose: concise completion feedback.
  - States: success, partial success, failure.

## Interaction and State Matrix
- Primary actions:
  - Continue (`Enter`), Back (`b`), Skip wizard (`s`), Quit (`q`), Confirm apply.
- Option movement:
  - Arrow keys or `j/k`; `Enter` accepts focused/default choice.
- Validation:
  - Inline message shown at failing step.
  - Keep failing value in context and show one corrective example.
- Error handling:
  - No stack trace in normal mode.
  - One-line cause + one-line fix.

### Mode Resolution Matrix
- TTY + no flags: wizard runs.
- TTY + `--wizard`: wizard runs.
- TTY + `--no-wizard`: wizard bypassed.
- Non-TTY + no wizard flags: non-interactive path.
- Non-TTY + `--wizard`: validation error.
- Non-TTY + `--preset ...`: validation error.
- `--wizard` + `--no-wizard`: validation error.
- `--preset ...` + `--no-wizard`: validation error.

### `--yes` Semantics
- Wizard enabled: auto-accept defaults for all steps and final confirmation.
- Wizard bypassed/non-TTY: accepted as no-op.
- `--yes` never suppresses validation errors.

## Preset Strategy (Refined)
Presets are valid only when wizard mode is active (auto or forced). They pre-seed wizard answers and are shown in plan summary before apply.

- `minimal`
  - Initialize `.tasque` files only.
  - No skill install/uninstall action.
- `standard`
  - Initialize `.tasque` files.
  - Install managed skill with default name and default targets.
  - No force overwrite.
- `full`
  - Same as `standard`.
  - Enable force overwrite for skill install.

Precedence inside wizard-enabled mode:
1. Explicit flags
2. Preset values
3. Built-in defaults

## Wizard Step Contract
- Step 1: Baseline init confirmation (always displayed unless `--yes` auto-advances).
- Step 2: Skill action selection (`install|uninstall|none`).
- Step 3: Skill details (targets/name/overwrite/dir overrides) only when needed.
- Step 4: Final resolved plan with explicit apply confirmation.

## Visual System
- Color roles:
  - Default text for structure.
  - Success/error/warn labels only where meaningful.
- Typography:
  - Terminal native, no ASCII art headers.
  - Keep line lengths under terminal width budgets.
- Spacing:
  - 1 blank line between prompt blocks.
  - 1 blank line before summary block.
- Iconography:
  - ASCII-safe markers only (`>`, `-`, `[ok]`, `[warn]`, `[error]`).

## Accessibility
- Keyboard navigation:
  - Full flow works without mouse.
  - Inputs mapped to simple keys (`Enter`, arrows, `j/k`, `b`, `s`, `q`).
- Focus order:
  - Strict top-to-bottom flow.
  - Active option always prefixed with `>`.
- Contrast:
  - Readable with ANSI color disabled.
- Assistive compatibility:
  - Avoid cursor repositioning and animated controls.
  - Keep output plain text and line-oriented.

## Content Notes
- Copy tone:
  - Direct and operational.
  - Prefer verb-first labels: `Install skill now?`.
- Empty/no-op copy:
  - `No skill action selected; initialized .tasque only.`
- Error copy style:
  - `Invalid value for --skill-targets: foo`
  - `Allowed: claude,codex,copilot,opencode,all`

## Acceptance Criteria (UX Handoff)
- Wizard behavior matches mode resolution matrix exactly.
- Wizard path can be completed end-to-end with keyboard only.
- Step count never exceeds 4.
- Final plan summary appears before writes in all wizard runs.
- `--yes` semantics match contract and never skip validation.
- Preset behavior is transparent in plan summary and deterministic.

## Test-Oriented Verification Guidance
- Add/maintain CLI tests for:
  - mode resolution matrix cases,
  - preset mapping behavior,
  - explicit flag over preset precedence,
  - `--yes` behavior in wizard and non-wizard modes,
  - concise, stable prompt/summary output across width tiers.
- Snapshot/golden tests should assert key lines, not full terminal framing, to reduce brittleness.
