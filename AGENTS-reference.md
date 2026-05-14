# Tasque Reference (V1)

This file contains detailed storage, task model, CLI contract, and project conventions.
Referenced from [AGENTS.md](./AGENTS.md).

## Storage Model
Git repositories default to sync-worktree mode:
- `tsq init` configures the `tsq-sync` branch/worktree unless `--sync-branch <name>` or `--worktree-name <name>` names a custom branch/worktree.
- data operations are redirected to the configured sync worktree
- legacy main-tree `.tasque` data migrates automatically when no `sync_branch` is configured
- fresh clones fetch the configured sync branch and create the worktree on first use
- `tsq sync` pushes the sync branch to `origin` and sets upstream automatically when needed
- the main worktree keeps `.tasque/config.json` as the pointer to the sync branch

Non-git directories use repo-local `.tasque/`:
- `.tasque/events.jsonl` (canonical source of truth, append-only)
- `.tasque/state.json` (derived cache, rebuildable, gitignored)
- `.tasque/tasks.jsonl` (legacy state-cache name; read-only fallback when `state.json` is absent; removal target)
- `.tasque/snapshots/` (replay checkpoints, local by default)
- `.tasque/config.json` (project settings)
- `.tasque/.lock` (ephemeral write lock)

Event fields:
- `id` (ULID, canonical)
- `event_id` (legacy alias accepted on read)
- `ts` (ISO datetime)
- `type`
- `actor`
- `task_id`
- `payload`

Event types:
- `task.created`
- `task.updated`
- `task.status_set`
- `task.claimed`
- `task.noted`
- `task.spec_attached`
- `task.superseded`
- `dep.added`
- `dep.removed`
- `link.added`
- `link.removed`

Read path:
- resolve configured sync worktree when `sync_branch` is set
- migrate legacy git repos to the default sync worktree when no `sync_branch` is set
- load latest snapshot (if any)
- replay event tail
- refresh `state.json` cache

Write path:
- append event(s)
- update projection
- periodically write snapshot

## Task Model
Task fields:
- `id` (`tsq-<number>` root, `<parent>.<n>` child); legacy `tsq-<8 crockford base32 chars>` IDs remain valid
- `alias` (kebab-case slug generated from the creation title; stable across title edits)
- `kind` (`task|feature|epic`)
- `title`
- `status` (`open|in_progress|blocked|deferred|closed|canceled`)
- `planning_state` (`needs_planning|planned`)
- `priority` (`0..3`)
- `assignee` (optional)
- `parent_id` (optional)
- `superseded_by` (optional)
- `duplicate_of` (optional)
- `replies_to` (optional)
- `discovered_from` (optional provenance metadata)
- `labels[]`
- `created_at`, `updated_at`, `closed_at`

Dependencies:
- edge: `child -> blocker` with `dep_type` (`blocks|starts_after`)
- semantics: only `blocks` participates in ready/cycle checks; `starts_after` is non-blocking ordering metadata

Relation types:
- `relates_to` (bidirectional)
- `replies_to`
- `duplicates`
- `supersedes`

## CLI Contract
- `tsq` (no args, TTY): open read-only TUI
- `tsq init [--wizard|--no-wizard] [--yes] [--preset <name>] [--sync-branch|--worktree-name <name>]`
- `tsq init --install-skill|--uninstall-skill [--skill-targets ...] [--skill-name <name>] [--force-skill-overwrite]`
- `tsq create <title...> [--kind ...] [-p ...] [--parent <id>] [--from-file tasks.md] [--description <text>] [--external-ref <ref>] [--discovered-from <id>] [--planned|--needs-plan] [--ensure] [--id <id>] [--body-file <path|->] [--force]`
- `tsq show <id> [--with-spec]`
- `tsq find ready [--lane <planning|coding>] [--assignee <name>] [--unassigned] [--kind ...] [--label ...] [--planning <needs_planning|planned>] [--tree [--full]]`
- `tsq find <blocked|open|in-progress|deferred|done|canceled> [filters...] [--tree [--full]]`
- `tsq find search <query> [--full]`
- `tsq find similar "<text>"`
- `tsq watch [--once] [--interval <seconds>] [--status <csv>] [--assignee <name>] [--tree] [--flat]`

Notes:
- For `find ready` and status-based `find` commands, `--full` is only valid with `--tree`. `--tree --full` keeps the full status set instead of applying the default tree status narrowing. `find search --full` remains valid without `--tree`.
- `--id <id>` accepts `tsq-<number>` or legacy `tsq-<8 crockford base32 chars>`.
- Commands that accept a task ID also accept exact aliases and unique alias prefixes unless `--exact-id` is used.
- `tsq find similar "<text>"` shows ranked duplicate candidates with scores and reasons.
- `tsq create` refuses similar open/in-progress/blocked/deferred tasks unless `--force` is passed.

`watch` renders the task tree by default for human output. Use `--tree` to explicitly request tree view or `--flat` for the compact list view. These options are mutually exclusive.
- `tsq tui [--once] [--interval <seconds>] [--status <csv>] [--assignee <name>] [--board|--epics]`
- `tsq stale [--days <n>] [--status <status>] [--assignee <name>] [--limit <n>]`
- `tsq doctor`
- `tsq repair [--fix] [--force-unlock]`
- `tsq edit <id> [--title ...] [--description ...] [--clear-description] [--priority ...] [--external-ref <ref>] [--clear-external-ref] [--discovered-from <id>] [--clear-discovered-from]`
- `tsq claim <id> [--assignee <a>] [--start] [--require-spec]`
- `tsq assign <id> --assignee <a>`
- `tsq start <id>`
- `tsq planned <id>`
- `tsq needs-plan <id>`
- `tsq open <id>`
- `tsq blocked <id>`
- `tsq defer <id> [--note <text>]`
- `tsq done <id...> [--note <text>]`
- `tsq reopen <id...> [--note <text>]`
- `tsq cancel <id...> [--note <text>]`
- `tsq orphans`
- `tsq spec <id> [--file <path> | --stdin | --text <markdown> | --show | --check] [--force]`
- `tsq block <task> by <blocker>`
- `tsq unblock <task> by <blocker>`
- `tsq order <later> after <earlier>`
- `tsq unorder <later> after <earlier>`
- `tsq deps <id> [--direction <up|down|both>] [--depth <n>]`
- `tsq relate <src> <dst>`
- `tsq unrelate <src> <dst>`
- `tsq duplicate <id> of <canonical-id> [--note <text>]`
- `tsq duplicates [--limit <n>]`
- `tsq merge <source-id...> --into <target-id> [--reason <text>] [--force] [--dry-run]`
- `tsq supersede <old-id> with <new-id> [--note <text>]`
- `tsq note <id> <text>`
- `tsq note <id> --stdin`
- `tsq notes <id>`
- `tsq label <id> <label>`
- `tsq unlabel <id> <label>`
- `tsq labels`
- `tsq history <id> [--limit <n>] [--type <event-type>] [--actor <name>] [--since <iso>]`
- `tsq sync [--no-push]`
- `tsq hooks install [--force]`
- `tsq hooks uninstall`
- `tsq migrate [--sync-branch|--worktree-name <name>]`
- `tsq merge-driver <ancestor> <ours> <theirs>`

Global options:
- `--format human|json`
- `--json` shorthand for `--format json`
- `--exact-id`

Status alias:
- `done -> closed`

Planning workflow guidance:
- Treat lifecycle `status` and `planning_state` as separate dimensions.
- `tsq find ready --lane planning` surfaces tasks that need planning work (`planning_state=needs_planning`).
- Planning-lane work should collaborate with the user and update specs/task body as needed before coding.
- `tsq find ready --lane coding` surfaces tasks already planned (`planning_state=planned`).
- Use `status=deferred` for valid work intentionally parked for later.

Exit codes:
- `0` success
- `1` validation/user error
- `2` storage/IO error
- `3` lock/concurrency failure

## JSON Output
All commands support:
```json
{
  "schema_version": 1,
  "command": "tsq ...",
  "ok": true,
  "data": {}
}
```

Error:
```json
{
  "schema_version": 1,
  "command": "tsq ...",
  "ok": false,
  "error": { "code": "VALIDATION_ERROR", "message": "..." }
}
```

## Durability + Integrity
- single-process lock via `.tasque/.lock` (`open wx`)
- lock timeout `3s`; jitter `20-80ms`
- stale cleanup only if same host and PID confirmed dead
- append-only event log
- atomic cache writes (`state.json.tmp-*` -> rename)
- startup recovery ignores one malformed trailing JSONL line with warning
- deterministic rebuild from snapshot + event tail
- reads legacy `.tasque/tasks.jsonl` only as a fallback state cache; do not write new data there

## Repo Conventions
- commit `.tasque/events.jsonl` and `.tasque/config.json`
- do not commit `.tasque/state.json`
- do not create or edit `.tasque/tasks.jsonl`
- snapshots optional to commit (default local-only)
- do not manually edit generated cache files

## Keep It Simple Rules
- one clear code path over abstractions
- no plugin system
- no backend interface layer until second backend exists
- target file size < 500 LOC
- use `cargo fmt` and `cargo clippy`
- keep strict typing

## Finishing tasks
- build the binary and place it in `~/.local/bin` so that it is available in the cli as tsq.
- run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --quiet`. Fix any issues that arise.
- use a fix forward approach and avoid unnecessary complexity of backward compatibility in mind. We are in active development.
- keep the codebase organized and modular. Refactor as needed to improve readability and maintainability.
  - Lookup if a refactor task already exists before creating a new one. If it doesn't create one so we can track it.
