# Tasque V1 Agent Spec

## Objective
Simple Beads-like tracker.
Local-first.
Git-friendly.
No DB.
No Dolt.
JSONL only.

## Reference
- Beads (behavior reference): `C:\Users\adityasharma\Projects\references\beads`

## Scope (V1)
- task create/read/update
- blockers + relation links
- ready detection
- audit trail
- stable machine output (`--json`)

## Non-Goals (V1)
- sqlite/dolt backends
- remote sync service
- background daemon
- multi-writer guarantees across machines

## Stack
- Runtime: native Rust binary (`tsq`)
- Language: Rust 2024
- CLI parser: `clap` derive
- Serialization/validation: `serde`/`serde_json` at JSONL boundaries plus typed domain validation
- Terminal output: TTY-aware render/style modules

## Storage Model (JSONL)
Git repos default to sync-worktree mode:
- `tsq init` configures `tsq-sync` unless `--sync-branch <name>` or `--worktree-name <name>` overrides it
- data operations redirect to the configured sync worktree
- legacy main-tree `.tasque` data migrates automatically when no `sync_branch` is configured
- fresh clones fetch the configured sync branch and create the worktree on first use
- `tsq sync` pushes the sync branch to `origin` and sets upstream automatically when needed
- main worktree keeps `.tasque/config.json` as the pointer to the sync branch

Non-git dirs use repo-local `.tasque/`:
- `.tasque/events.jsonl` (source of truth, append-only)
- `.tasque/state.json` (derived cache; rebuildable)
- `.tasque/config.json` (project settings)

Each event: one JSON object/line.
Required fields:
- `id` (event id)
- `ts` (ISO datetime)
- `type` (event type)
- `actor` (agent/user)
- `payload` (typed object)

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
- load `state.json` if present + fresh
- else replay `events.jsonl`
- on write: append event, update cache

## Task Model
Task fields:
- `id` (`tsq-<hash>` root, `<parent>.<n>` child)
- `title`
- `status` (`open|in_progress|blocked|deferred|closed|canceled`)
- `priority` (`0..3`)
- `assignee` (optional)
- `parent_id` (optional)
- `created_at`, `updated_at`
- `labels[]` (optional)

Links:
- dependency edge: `child -> parent_blocker`
- relation edge kinds: `relates_to|duplicates|supersedes|replies_to`

## Ready Semantics
`ready` if:
- task status in `open|in_progress`
- task has zero open blockers
- task not `canceled|closed`

Open blocker:
- linked dependency target exists
- target status not in `closed|canceled`

## CLI Contract (V1)
- `tsq` (no args, TTY): open read-only TUI
- `tsq init [--wizard|--no-wizard] [--yes] [--preset <name>] [--sync-branch|--worktree-name <name>]`
- `tsq init --install-skill|--uninstall-skill [--skill-targets ...] [--skill-name <name>] [--force-skill-overwrite]`
- `tsq create [<title>] [--child <title> ...] [--kind ...] [-p 0..3] [--parent <id>] [--description <text>] [--external-ref <ref>] [--discovered-from <id>] [--planning <needs_planning|planned>] [--needs-planning] [--ensure] [--id <tsq-xxxxxxxx>] [--body-file <path|->]`
- `tsq show <id>`
- `tsq list [--status ...] [--assignee ...] [--unassigned] [--external-ref <ref>] [--discovered-from <id>] [--kind ...] [--label ...] [--label-any ...] [--created-after <iso>] [--updated-after <iso>] [--closed-after <iso>] [--id <id,...>] [--planning <needs_planning|planned>] [--dep-type <blocks|starts_after>] [--dep-direction <in|out|any>] [--tree] [--full]`
- `tsq search <query>`
- `tsq ready [--lane <planning|coding>]`
- `tsq watch [--once] [--interval <seconds>] [--status <csv>] [--assignee <name>] [--tree]`
- `tsq tui [--once] [--interval <seconds>] [--status <csv>] [--assignee <name>] [--board|--epics]`
- `tsq stale [--days <n>] [--status <status>] [--assignee <name>] [--limit <n>]`
- `tsq doctor`
- `tsq repair [--fix] [--force-unlock]`
- `tsq update <id> [--title ...] [--description ...] [--clear-description] [--status ...] [--priority ...] [--external-ref <ref>] [--clear-external-ref] [--discovered-from <id>] [--clear-discovered-from] [--planning <needs_planning|planned>]`
- `tsq update <id> --claim [--assignee <a>] [--require-spec]`
- `tsq close <id...> [--reason <text>]`
- `tsq reopen <id...>`
- `tsq orphans`
- `tsq spec attach <id> [source] [--file <path> | --stdin | --text <markdown>]`
- `tsq spec check <id>`
- `tsq dep add <child> <blocker> [--type <blocks|starts_after>]`
- `tsq dep tree <id> [--direction <up|down|both>] [--depth <n>]`
- `tsq dep remove <child> <blocker> [--type <blocks|starts_after>]`
- `tsq link add <src> <dst> --type <relates_to|replies_to|duplicates|supersedes>`
- `tsq link remove <src> <dst> --type <relates_to|replies_to|duplicates|supersedes>`
- `tsq duplicate <id> --of <canonical-id> [--reason <text>]`
- `tsq duplicates [--limit <n>]`
- `tsq merge <source-id...> --into <target-id> [--reason <text>] [--force] [--dry-run]`
- `tsq supersede <old-id> --with <new-id> [--reason <text>]`
- `tsq note add <id> <text>`
- `tsq note list <id>`
- `tsq label add <id> <label>`
- `tsq label remove <id> <label>`
- `tsq label list`
- `tsq history <id> [--limit <n>] [--type <event-type>] [--actor <name>] [--since <iso>]`
- `tsq sync [--no-push]`
- `tsq hooks install [--force]`
- `tsq hooks uninstall`
- `tsq migrate [--sync-branch|--worktree-name <name>]`
- `tsq merge-driver <ancestor> <ours> <theirs>`

Global options:
- `--json`
- `--exact-id`

Exit codes:
- `0` success
- `1` validation/user error
- `2` storage/IO error
- `3` lock/concurrency failure

## Concurrency + Integrity
- single-process write lock: `.tasque/.lock` (`open wx`, short retry)
- append-only writes
- atomic cache writes (`state.json.tmp` -> rename)
- startup recovery: ignore malformed trailing JSONL line; warn once

## Repo Conventions
- commit `.tasque/events.jsonl` + `.tasque/config.json`
- do not commit `.tasque/state.json`
- do not create or edit `.tasque/tasks.jsonl`
- no manual edits to generated cache

## Build Plan
1. maintain Rust CLI/TUI feature parity
2. keep event append/read/replay durable
3. keep command behavior stable under `--json`
4. keep release/npm packaging aligned to `Cargo.toml`
5. run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --quiet` before handoff

## Keep It Simple Rules
- prefer one clear code path over abstractions
- no backend plugin system in current scope
- no backend interface layer until second backend exists
- file size target: <500 LOC per file
