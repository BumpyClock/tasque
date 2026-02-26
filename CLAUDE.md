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
- Runtime: Bun (latest)
- Language: TypeScript (`strict`)
- CLI parser: `commander` (simple, stable)
- Validation: `zod` + `TsqError` (event-line schema validation at JSONL parse boundaries; projector/service still perform domain validation)
- Terminal output: `picocolors` + minimal table util

## Storage Model (JSONL)
Repo-local `.tasque/`:
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
- `dep.added`
- `dep.removed`
- `link.added`
- `link.removed`

Read path:
- load `state.json` if present + fresh
- else replay `events.jsonl`
- on write: append event, update cache

## Task Model
Task fields:
- `id` (`tsq-<hash>` root, `<parent>.<n>` child)
- `title`
- `status` (`open|in_progress|blocked|closed|canceled`)
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
- `tsq init`
- `tsq create [<title>] [--child <title> ...] [-p 0..3] [--parent <id>] [--ensure] [--json]`
- `tsq show <id> [--json]`
- `tsq list [--status <s>] [--assignee <a>] [--json]`
- `tsq ready [--json]`
- `tsq update <id> [--title ...] [--status ...] [--priority ...] [--json]`
- `tsq update <id> --claim [--assignee <a>] [--json]`
- `tsq dep add <child> <blocker> [--json]`
- `tsq dep remove <child> <blocker> [--json]`
- `tsq link add <src> <dst> --type <relation> [--json]`

Exit codes:
- `0` success
- `1` validation/user error
- `2` storage/IO error

## Concurrency + Integrity
- single-process write lock: `.tasque/.lock` (`open wx`, short retry)
- append-only writes
- atomic cache writes (`state.json.tmp` -> rename)
- startup recovery: ignore malformed trailing JSONL line; warn once

## Repo Conventions
- commit `.tasque/events.jsonl` + `.tasque/config.json`
- optional: commit `state.json` (or regenerate in CI)
- no manual edits to generated cache

## Build Plan
1. bootstrap Bun TS CLI skeleton
2. implement event append/read + replay
3. implement task commands (`init/create/show/update/list`)
4. implement deps/links + ready logic
5. add `--json` stable output
6. add tests (ID generation, replay, ready, deps)

## Keep It Simple Rules
- prefer one clear code path over abstractions
- no plugin system in V1
- no backend interface layer until second backend exists
- file size target: <500 LOC per file
