# Tasque Agent Guide (V1 Shipped)

- assume tsq is already built and installed in path. if not build and install it. 
- use parallel sub-agents and agent teams for implementation.

## Objective
Simple local-first tracker for local agent work, inspired by Beads patterns.
Local-first. Git-friendly. JSONL-backed.
Durable across restarts and context compaction.

## Reference
- Inspiration reference: `C:\Users\adityasharma\Projects\references\beads`

## Scope (Current)
- task/feature/epic create/read/update
- typed dependencies (`blocks`, `starts_after`)
- relation links
- duplicate workflow (`duplicate`, `duplicates` dry-run scaffold)
- merge workflow (`merge` with `--force` and `--dry-run`)
- ready detection
- lane-aware ready detection (`--lane planning|coding`)
- planning state tracking (`planning_state`)
- deferred lifecycle status for parked work
- atomic claim
- optional claim spec gate (`--require-spec`)
- spec attach/check workflow
- supersede workflow
- orphans reporting (`tsq orphans`, read-only)
- append-only audit trail
- stable machine output (`--json`)
- tree list view (`tsq list --tree`)
- skill install/uninstall via `tsq init`

## Non-Goals
- sqlite/dolt backends
- remote sync service
- background daemon
- multi-machine consistency

## Stack
- Runtime: Bun
- Language: TypeScript (`strict`)
- CLI: `commander`
- Validation: `zod`
- Output: `picocolors`

## Storage Model
Repo-local `.tasque/`:
- `.tasque/events.jsonl` (canonical source of truth, append-only)
- `.tasque/state.json` (derived cache, rebuildable, gitignored)
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
- load latest snapshot (if any)
- replay event tail
- refresh `state.json` cache

Write path:
- append event(s)
- update projection
- periodically write snapshot

## Task Model
Task fields:
- `id` (`tsq-<8 crockford base32 chars>` root, `<parent>.<n>` child)
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
- `tsq init`
- `tsq init --install-skill|--uninstall-skill [--skill-targets ...]`
- `tsq create "Title" [--kind ...] [-p ...] [--parent <id>] [--external-ref <ref>] [--discovered-from <id>] [--planning <needs_planning|planned>] [--needs-planning] [--id <tsq-xxxxxxxx>] [--body-file <path|->]`
- `tsq show <id>`
- `tsq list [--status ...] [--assignee ...] [--external-ref <ref>] [--discovered-from <id>] [--kind ...] [--planning <needs_planning|planned>] [--dep-type <blocks|starts_after>] [--dep-direction <in|out|any>] [--tree]`
- `tsq ready [--lane <planning|coding>]`
- `tsq watch [--once] [--interval <seconds>] [--status <csv>] [--assignee <name>] [--tree]`
- `tsq stale [--days <n>] [--status <status>] [--assignee <name>] [--limit <n>]`
- `tsq doctor`
- `tsq update <id> [--title ...] [--status ...] [--priority ...] [--external-ref <ref>] [--clear-external-ref] [--discovered-from <id>] [--clear-discovered-from] [--planning <needs_planning|planned>]`
- `tsq update <id> --claim [--assignee <a>] [--require-spec]`
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

Global options:
- `--json`
- `--exact-id`

Status alias:
- `done -> closed`

Planning workflow guidance:
- Treat lifecycle `status` and `planning_state` as separate dimensions.
- `tsq ready --lane planning` surfaces tasks that need planning work (`planning_state=needs_planning`).
- Planning-lane work should collaborate with the user and update specs/task body as needed before coding.
- `tsq ready --lane coding` surfaces tasks already planned (`planning_state=planned`).
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

## Repo Conventions
- commit `.tasque/events.jsonl` and `.tasque/config.json`
- do not commit `.tasque/state.json`
- snapshots optional to commit (default local-only)
- do not manually edit generated cache files

## Keep It Simple Rules
- one clear code path over abstractions
- no plugin system
- no backend interface layer until second backend exists
- target file size < 500 LOC
- use Biome for format/lint
- keep strict typing

## Finishing tasks
- build the binary and place it in `~/.local/bin` so that it is available in the cli as tsq.
- run `bun run doctor` to ensure lint and formatting pass. Fix any issues that arise.
- use a fix forward approach and avoid unnecessary complexity of backward compatibility in mind. We are in active development.
- keep the codebase organized and modular. Refactor as needed to improve readability and maintainability. 
  - Lookup if a refactor task already exists before creating a new one. If it doesn't create one so we can track it.
