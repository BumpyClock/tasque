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

## CLI Contract
Current verb-first command contract lives in [AGENTS-reference.md](./AGENTS-reference.md).
Keep this file high-level to avoid stale command matrices.

Canonical examples:
- `tsq create "Fix auth redirect"`
- `tsq create --from-file tasks.md`
- `tsq find ready --lane coding`
- `tsq edit <id> --title "New title"`
- `tsq claim <id> --start --require-spec`
- `tsq block <task> by <blocker>`
- `tsq order <later> after <earlier>`
- `tsq relate <src> <dst>`
- `tsq spec <id> --file spec.md`
- `tsq spec <id> --check`
- `tsq note <id> "decision recorded"`
- `tsq notes <id>`
- `tsq label <id> cli`
- `tsq unlabel <id> cli`
- `tsq labels`
- `tsq done <id> --note "merged"`

Global options:
- `--format human|json`
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
