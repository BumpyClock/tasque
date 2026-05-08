# Verb-First CLI Ergonomics Redesign

## Overview

Tasque CLI should match how coding agents naturally phrase task work. Current commands expose storage/domain mechanics (`note add`, `dep add`, `update --status`) and logs show repeated help probes, invalid flags, and multi-command workflows for simple intents.

This redesign makes `tsq` verb-first and sentence-like. It intentionally breaks the old CLI surface during active development. CLI help, README, `AGENTS-reference.md`, and the Tasque skill become the canonical guidance.

## Goals

- Make common task actions map to one natural command.
- Replace collection reads (`list`, `ready`, `search`) with `find`.
- Remove nested command grammar where it caused friction.
- Add first-class spec content retrieval for sync-worktree specs.
- Support batch create from Markdown bullets.
- Keep machine-readable output available without forcing agents to use it.
- Provide helpful errors that point from removed commands to new commands.

## Non-Goals

- Preserve old command compatibility.
- Add JSON/YAML task-file parsing.
- Redesign the event model or projector.
- Change storage location for specs.
- Build a new TUI workflow.

## Command Grammar

Canonical commands:

```bash
tsq create "Title" --kind task -p 2 --needs-plan
tsq create --parent <id> "Child A" "Child B"
tsq create --parent <id> --from-file tasks.md
tsq edit <id> --title "New title"
tsq edit <id> --description "New description"
tsq edit <id> --priority 1

tsq note <id> "text"
tsq note <id> --stdin
tsq notes <id>

tsq spec <id> --text "markdown"
tsq spec <id> --file spec.md
tsq spec <id> --file spec.md --force
tsq spec <id> --stdin
tsq spec <id> --show
tsq spec <id> --check

tsq show <id>
tsq show <id> --with-spec
tsq find ready --lane coding
tsq find blocked
tsq find open
tsq find search "search terms"
```

Lifecycle commands:

```bash
tsq claim <id> --assignee codex --start --require-spec
tsq start <id>
tsq planned <id>
tsq needs-plan <id>
tsq defer <id> --note "waiting"
tsq done <id...> --note "verified"
tsq reopen <id...> --note "regressed"
tsq cancel <id...> --note "obsolete"
```

Relation commands:

```bash
tsq block <task> by <blocker>
tsq unblock <task> by <blocker>
tsq order <later> after <earlier>
tsq unorder <later> after <earlier>
tsq relate <a> <b>
tsq unrelate <a> <b>
tsq duplicate <id> of <canonical> --note "same issue"
tsq supersede <old> with <new> --note "new approach"
```

Output commands:

```bash
tsq --format json find ready --lane coding
tsq --json show <id> --with-spec
```

`--format json` is canonical. `--json` remains accepted as shorthand. Skill guidance should say to use structured output when scripting or parsing, not for every inspection command.

## Public Command Surface

The redesign breaks old command names that conflicted with natural agent phrasing, but keeps root commands that are already clear operational verbs.

| Current surface | New surface | Decision |
|---|---|---|
| `create` | `create` | Keep, change child/batch grammar |
| `show` | `show` | Keep, add `--with-spec` and optional note display |
| `list` | `find` | Replace |
| `ready` | `find ready` | Replace |
| `search` | `find search` | Replace |
| `update` | `edit` plus lifecycle verbs | Replace |
| `close` | `done` | Replace |
| `reopen` | `reopen` | Keep, add `--note` |
| `note add` | `note` | Replace |
| `note list` | `notes` | Replace |
| `spec attach` | `spec` | Replace |
| `spec check` | `spec --check` | Replace |
| `dep add/remove/tree` | `block`, `unblock`, `order`, `unorder`, `deps` | Replace |
| `link add/remove` | `relate`, `unrelate` | Replace |
| `label add/remove/list` | `label`, `unlabel`, `labels` | Replace |
| `duplicate` | `duplicate <id> of <canonical>` | Keep root, change grammar |
| `duplicates` | `duplicates` | Keep |
| `merge` | `merge` | Keep |
| `supersede` | `supersede <old> with <new>` | Keep root, change grammar |
| `history` | `history` | Keep |
| `stale` | `stale` | Keep |
| `watch` | `watch` | Keep |
| `orphans` | `orphans` | Keep |
| `doctor` | `doctor` | Keep |
| `repair` | `repair` | Keep |
| `sync` | `sync` | Keep |
| `hooks` | `hooks` | Keep |
| `init` | `init` | Keep |
| `migrate` | `migrate` | Keep |
| `merge-driver` | `merge-driver` | Keep |

New helper commands:

```bash
tsq label <id> <label>
tsq unlabel <id> <label>
tsq labels
tsq deps <id> --direction up|down|both --depth <n>
```

`edit` owns metadata changes that are not lifecycle transitions:

```bash
tsq edit <id> --title "New title"
tsq edit <id> --description "New description"
tsq edit <id> --clear-description
tsq edit <id> --priority 1
tsq edit <id> --external-ref <ref>
tsq edit <id> --clear-external-ref
tsq edit <id> --discovered-from <id>
tsq edit <id> --clear-discovered-from
```

Planning state is intentionally not on `edit`; use `planned` and `needs-plan`.

## Batch Create

`tsq create --from-file tasks.md` accepts Markdown bullet list only:

```md
- Add parser tests
  - Cover nested task hierarchy
  - Cover invalid indentation
- Wire CLI command
- Update skill docs
```

Rules:

- Each `- ` bullet becomes one task title.
- Nested bullets use exactly two spaces per depth.
- Nested bullets create real parent/child hierarchy.
- Checkbox bullets `- [ ] title` and `- [x] title` are accepted and the checkbox marker is stripped.
- Blank lines are ignored.
- CRLF and LF line endings are accepted.
- Non-bullet content is rejected with a validation error that includes the line number.
- `--parent` makes top-level bullets children of the given parent.
- Shared flags apply to every created task: `--kind`, `-p`, `--planned`, and `--needs-plan`.
- Label assignment during create is out of scope for this redesign. Use `tsq label <id> <label>` after creation.

Create argument rules:

- Positional titles are variadic: `titles: Vec<String>`.
- `--from-file` conflicts with positional titles.
- `--from-file` conflicts with `--id`.
- `--description`, `--body-file`, and `--id` are valid only when creating exactly one task.
- `--ensure` applies to every created title and conflicts with `--id`.
- `--planned` and `--needs-plan` conflict.

The Tasque skill must include this exact format so agents can create a temporary `tasks.md` confidently.

## Spec Access

Specs are stored under `.tasque/specs/<id>/spec.md`, which may live in the `tsq-sync` worktree. Agents often see the spec path but cannot find the file from the main project worktree. The CLI must provide content access.

Behavior:

- `tsq spec <id> --show` resolves the Tasque repo root and sync worktree, then prints the Markdown spec content.
- `tsq show <id> --with-spec` prints normal task details plus spec content.
- Human output wraps Markdown content in clear delimiters:
  - `--- spec: <path> ---`
  - `--- end spec ---`
- JSON output includes `data.spec.path`, `data.spec.fingerprint`, and `data.spec.content` only when `--show` or `--with-spec` requested.
- `--force` keeps existing conflict behavior for replacing a spec with a different fingerprint.
- Missing spec returns a validation error with a hint such as `tsq spec <id> --file spec.md`.
- Fingerprint/check behavior stays with `tsq spec <id> --check`.

## Find Semantics

`find` replaces collection reads:

- `tsq find ready --lane planning|coding` replaces `ready`.
- `tsq find blocked` replaces `list --status blocked`.
- `tsq find open`, `tsq find in-progress`, `tsq find deferred`, `tsq find done`, and `tsq find canceled` replace status list flows.
- `tsq find search "query"` replaces search.
- `find ready` supports `--lane planning|coding`, `--assignee`, `--unassigned`, `--kind`, `--label`, `--planning`, `--tree`, and `--full`.
- `find blocked|open|in-progress|deferred|done|canceled` supports `--assignee`, `--unassigned`, `--kind`, `--label`, `--label-any`, `--planning`, `--external-ref`, `--discovered-from`, `--created-after`, `--updated-after`, `--closed-after`, `--id`, `--dep-type`, `--dep-direction`, `--tree`, and `--full`.
- `find search "query"` supports search query text and `--full`.
- Literal search for reserved words uses `find search`: `tsq find search "ready"`.

`show <id>` remains for single-task detail.

## Lifecycle Semantics

Lifecycle verbs map to current service operations:

- `claim <id>` claims a task and sets `in_progress`, matching current claim semantics.
- `claim <id> --start` is accepted for readability but is equivalent to `claim <id>`.
- `assign <id> --assignee <name>` sets assignee without changing status.
- `start <id>` sets `in_progress`.
- `planned <id>` sets `planning_state=planned`.
- `needs-plan <id>` sets `planning_state=needs_planning`.
- `open <id>` sets `open`.
- `blocked <id>` sets lifecycle status `blocked`; dependency blocking uses `block <task> by <blocker>`.
- `defer <id> --note ...` adds note and sets `deferred`.
- `done <id...> --note ...` adds optional note and closes all ids.
- `reopen <id...> --note ...` adds optional note and reopens all ids.
- `cancel <id...> --note ...` adds optional note and cancels all ids.

If a lifecycle command performs both note and status change, it should emit one clear human summary and stable JSON containing all affected task ids and note ids where applicable.

Status transition matrix:

| Command | Result |
|---|---|
| `claim <id>` | `status=in_progress`, assignee set if provided |
| `assign <id> --assignee <name>` | assignee set, status unchanged |
| `start <id>` | `status=in_progress` |
| `open <id>` | `status=open` |
| `blocked <id>` | `status=blocked` |
| `defer <id>` | `status=deferred` |
| `done <id...>` | `status=closed` |
| `reopen <id...>` | `status=open` |
| `cancel <id...>` | `status=canceled` |

## Relation Semantics

Sentence tokens are required:

- `block <task> by <blocker>` creates a `blocks` dependency.
- `unblock <task> by <blocker>` removes a `blocks` dependency.
- `order <later> after <earlier>` creates a `starts_after` dependency.
- `unorder <later> after <earlier>` removes a `starts_after` dependency.
- `relate <a> <b>` creates a bidirectional relation.
- `unrelate <a> <b>` removes a bidirectional relation.
- `duplicate <id> of <canonical>` marks a duplicate.
- `supersede <old> with <new>` supersedes old with new.
- `deps <id> --direction up|down|both --depth <n>` shows dependency tree.

Malformed sentence tokens should fail with a corrected command example.

## Note Semantics

`note` writes one note. `notes` lists notes.

Rules:

- `tsq note <id> "text"` accepts single-line or shell-quoted multiline text.
- `tsq note <id> --stdin` reads note text from standard input.
- Positional text and `--stdin` conflict.
- Empty or whitespace-only notes are rejected.
- JSON note results include `data.task_id`, `data.note.event_id`, `data.note.text`, `data.note.actor`, and `data.note.ts`.

`show <id>` does not print full note history. Full note listing uses `notes <id>`.

## Global Output Contract

Global output options:

```bash
tsq --format human show <id>
tsq --format json show <id>
tsq --json show <id>
```

Rules:

- `--format` accepts `human` and `json`.
- Default is `human`.
- `--json` is shorthand for `--format json`.
- Combining `--json` and `--format human` is a validation error.
- Global options are accepted before or after any subcommand.
- Parse errors emit the standard JSON error envelope when JSON output was requested.
- No-args TTY behavior remains the read-only TUI. No-args with `--format json` returns a JSON validation error.

## Error UX

Removed commands fail with direct migration hints:

- `tsq note add` -> `tsq note <id> "text"`
- `tsq dep add` -> `tsq block <task> by <blocker>` or `tsq order <later> after <earlier>`
- `tsq list`, `tsq ready`, `tsq search` -> `tsq find ...`
- `tsq update --status closed` -> `tsq done <id>`
- `tsq close` -> `tsq done <id>`

General parse errors should prefer corrected snippets over generic `unexpected argument` output when the intent is recognizable.

This requires a custom parse-error mapper for removed roots/subcommands before falling back to default clap rendering. Human and JSON modes both need migration-hint tests. Removed-command errors should use exit code `1` because the command is recognized as a user validation issue, not a storage or runtime failure.

## Implementation Plan Shape

Most work should happen in the parser/CLI layer:

- Replace `CommandKind` with verb-first commands.
- Reuse service functions for create, note, spec, update, close, deps, links, duplicate, supersede, and query behavior.
- Add a service helper for resolved spec content reads.
- Add a Markdown bullet parser for `create --from-file`.
- Add render support for spec content and multi-action summaries.
- Update JSON envelopes for new command names and requested spec content.
- Remove old command implementations from the public CLI.
- Add a parse-error mapper for removed commands and common old subcommand shapes.

## Docs And Skill Updates

Update:

- `AGENTS-reference.md` CLI contract.
- `README.md` examples.
- Tasque skill template and embedded skill references.
- CLI help examples for create/note/spec/find/lifecycle/relation flows.

Skill guidance:

- Use verb-first commands.
- Use `--format json` only when scripting/parsing.
- Use `tsq spec <id> --show` or `tsq show <id> --with-spec` when task specs are needed.
- Use Markdown bullet `tasks.md` for batch create.

## Test Plan

Add/update CLI contract tests:

- `tsq note <id> "text"` and `tsq note <id> --stdin`.
- `tsq create --from-file` happy path and reject cases.
- `tsq find ready/open/blocked/search`.
- `tsq claim --start`, `tsq done --note`, multi-id `done`.
- `tsq spec <id> --show`.
- `tsq show <id> --with-spec`.
- JSON includes `spec.content` only when requested.
- Relation sentence token commands and malformed token errors.
- Removed commands fail with migration hints.
- `--format json`, `--json`, and conflict handling with `--format human`.
- `--exact-id` propagation through all new commands that resolve task ids.
- Sync-worktree spec read via `spec --show` and `show --with-spec`.
- `notes <id>` listing.
- `edit` metadata replacement for old `update` behavior.
- Relation removals via `unblock`, `unorder`, and `unrelate`.
- Help output includes canonical examples.
- Embedded Tasque skill includes verb-first commands and `tasks.md` bullet format.

Run full gate before handoff:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --quiet
```
