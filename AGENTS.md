# Tasque V1 Agent Spec

## Objective
Simple Beads-like tracker.
Local-first.
Git-friendly.
No DB.
No Dolt.
JSONL only.
Durable across restarts + context compaction.

## Reference
- Beads behavior reference: `C:\Users\adityasharma\Projects\references\beads`

## Scope (V1)
- task/feature/epic create/read/update
- blocker dependencies
- relation links
- ready detection
- atomic claim
- supersede workflow
- append-only audit trail
- stable machine output (`--json`)

## Non-Goals (V1)
- sqlite/dolt backends
- remote sync service
- background daemon
- multi-machine consistency

## Stack
- Runtime: Bun (latest)
- Language: TypeScript (`strict`)
- CLI parser: `commander`
- Validation: `zod`
- Output: `picocolors` + minimal table util

## Storage Model (JSONL)
Repo-local `.tasque/`:
- `.tasque/events.jsonl` (source of truth, append-only)
- `.tasque/snapshots/` (periodic projection checkpoints)
- `.tasque/state.json` (latest derived cache; rebuildable; gitignored)
- `.tasque/config.json` (project settings)
- `.tasque/.lock` (ephemeral write lock)

Each event: one JSON object/line.
Required fields:
- `event_id` (ULID)
- `ts` (ISO datetime)
- `type` (event type)
- `actor` (agent/user)
- `task_id` (primary subject)
- `payload` (typed object)

Event types:
- `task.created`
- `task.updated`
- `task.claimed`
- `task.superseded`
- `dep.added`
- `dep.removed`
- `link.added`
- `link.removed`

Read path:
- load newest snapshot (if any)
- replay events after snapshot offset
- refresh `state.json` cache

Write path:
- append event(s)
- update projection
- periodically persist snapshot (e.g., every N events)

## Task Model
Task fields:
- `id` (`tsq-<hash6>` root, `<parent>.<n>` child)
- `kind` (`task|feature|epic`)
- `title`
- `status` (`open|in_progress|blocked|closed|canceled`)
- `priority` (`0..3`)
- `assignee` (optional)
- `parent_id` (optional)
- `superseded_by` (optional)
- `duplicate_of` (optional)
- `replies_to` (optional)
- `labels[]` (optional)
- `created_at`, `updated_at`, `closed_at` (when status `closed`)

Dependencies:
- edge: `child -> blocker`
- type: `blocks` only in V1

Relation link types:
- `relates_to`
- `replies_to`
- `duplicates`
- `supersedes`

## Ready Semantics
`ready` if:
- task status in `open|in_progress`
- task has zero open blockers

Open blocker:
- linked dependency target exists
- target status not in `closed|canceled`

Notes:
- `blocked` status can be set manually
- closing a blocker (including via supersede) unblocks dependents

## CLI Contract (V1)
- `tsq init`
- `tsq create "Title" [--kind task|feature|epic] [-p 0..3] [--parent <id>] [--json]`
- `tsq show <id> [--json]`
- `tsq list [--status <s>] [--assignee <a>] [--kind <k>] [--json]`
- `tsq ready [--json]`
- `tsq update <id> [--title ...] [--status ...] [--priority ...] [--json]`
- `tsq update <id> --claim [--assignee <a>] [--json]`
- `tsq dep add <child> <blocker> [--json]`
- `tsq dep remove <child> <blocker> [--json]`
- `tsq link add <src> <dst> --type <relates_to|replies_to|duplicates|supersedes> [--json]`
- `tsq link remove <src> <dst> --type <relates_to|replies_to|duplicates|supersedes> [--json]`
- `tsq supersede <old-id> --with <new-id> [--reason <text>] [--json]`

ID resolution:
- default: partial IDs resolved if unique
- ambiguity: hard error with candidate list
- `--exact-id`: disable partial resolution

Status aliasing:
- CLI accepts `done` alias
- normalize to canonical `closed`

Exit codes:
- `0` success
- `1` validation/user error
- `2` storage/IO error
- `3` lock-timeout/concurrency failure

## JSON Output Contract
All commands use envelope:
```json
{
  "schema_version": 1,
  "command": "tsq ready",
  "ok": true,
  "data": {}
}
```
Error envelope:
```json
{
  "schema_version": 1,
  "command": "tsq ready",
  "ok": false,
  "error": {"code": "VALIDATION_ERROR", "message": "..."}
}
```

## Concurrency + Integrity
- single-process write lock: `.tasque/.lock` (`open wx`)
- lock timeout: 3s
- retry jitter: 20-80ms
- stale lock threshold: 30s
- stale cleanup only when same host + PID confirmed dead
- otherwise fail-safe: keep lock, timeout, return lock error
- append-only writes
- atomic cache writes (`state.json.tmp` -> rename)
- startup recovery: ignore malformed trailing JSONL line; warn once

## Compaction Durability Strategy
- `events.jsonl` remains canonical source of truth
- compaction creates snapshot checkpoints; does not rewrite semantic history
- replay remains deterministic from snapshot + event tail
- never destroy ability to rebuild current state after restart

## Locked V1 Decisions (2026-02-17)
1. Bun + TypeScript for V1.
2. Root IDs fixed 6-char hash suffix (`tsq-xxxxxx`), collision retry via nonce.
3. Child IDs append-only hierarchical (`parent.n`), no gap reuse.
4. `relates_to` is bidirectional (maintain both directions atomically).
5. Keep generic `link add/remove` and dedicated workflow command `supersede`.
6. `link add --type supersedes|duplicates` is metadata-only.
7. `supersede` is canonical workflow transition: set `superseded_by`, set source `status=closed`, keep replacement unchanged.
8. `supersede` does not auto-rewire dependencies.
9. `supersede` validity checks: both IDs exist, resolved IDs differ, no self-supersede.
10. Claim policy strict CAS: only unassigned tasks; no force-claim in V1.
11. Canonical status is `closed`; CLI alias `done -> closed`.
12. `state.json` gitignored; regenerate locally from snapshot+events.
13. JSON envelope includes `schema_version: 1` for all command outputs.
14. Event IDs use ULID.
15. Reject dependency cycles; for link graph only block self-edge in V1.
16. Actor source: `TSQ_ACTOR` env -> git `user.name` -> OS user -> `unknown`.

## Repo Conventions
- commit `.tasque/events.jsonl` + `.tasque/config.json`
- do not commit `.tasque/state.json`
- snapshots optional to commit; default local-only in V1
- no manual edits to generated cache

## Build Plan
1. bootstrap Bun TS CLI skeleton
2. implement lock + event append/read + replay
3. implement projection cache + periodic snapshots
4. implement task commands (`init/create/show/update/list/claim`)
5. implement deps + ready logic + dep cycle checks
6. implement `link add/remove` relation edges (bidirectional `relates_to`)
7. implement `supersede` command wrapper semantics
8. add `--json` envelope + schema_version golden tests

## Keep It Simple Rules
- prefer one clear code path over abstractions
- no plugin system in V1
- no backend interface layer until second backend exists
- file size target: <500 LOC per file
- use Biome for formatting/linting
- enforce Biome in CI
- maintain `doctor` script for format/lint checks
- strict type safety
- use zod for schema validation
