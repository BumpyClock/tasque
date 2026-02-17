# Tasque Agent Guide (V1 Shipped)

- assume tsq is already built and installed in path. if not build and install it. 
- use parallel sub-agents and agent teams for implementation.

## Objective
Simple Beads-inspired tracker for local agent work.
Local-first. Git-friendly. JSONL-backed.
Durable across restarts and context compaction.

## Reference
- Beads behavior reference: `C:\Users\adityasharma\Projects\references\beads`

## Scope (Current)
- task/feature/epic create/read/update
- blocker dependencies
- relation links
- ready detection
- atomic claim
- supersede workflow
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
- `.tasque/tasks.jsonl` (derived cache, rebuildable, gitignored)
- `.tasque/snapshots/` (replay checkpoints, local by default)
- `.tasque/config.json` (project settings)
- `.tasque/.lock` (ephemeral write lock)

Event fields:
- `event_id` (ULID)
- `ts` (ISO datetime)
- `type`
- `actor`
- `task_id`
- `payload`

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
- load latest snapshot (if any)
- replay event tail
- refresh `tasks.jsonl` cache

Write path:
- append event(s)
- update projection
- periodically write snapshot

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
- `labels[]`
- `created_at`, `updated_at`, `closed_at`

Dependencies:
- edge: `child -> blocker`
- semantics: blocker open unless `closed|canceled`

Relation types:
- `relates_to` (bidirectional)
- `replies_to`
- `duplicates`
- `supersedes`

## CLI Contract
- `tsq init`
- `tsq init --install-skill|--uninstall-skill [--skill-targets ...]`
- `tsq create "Title" [--kind ...] [-p ...] [--parent <id>]`
- `tsq show <id>`
- `tsq list [--status ...] [--assignee ...] [--kind ...] [--tree]`
- `tsq ready`
- `tsq doctor`
- `tsq update <id> [--title ...] [--status ...] [--priority ...]`
- `tsq update <id> --claim [--assignee <a>]`
- `tsq dep add <child> <blocker>`
- `tsq dep remove <child> <blocker>`
- `tsq link add <src> <dst> --type <relates_to|replies_to|duplicates|supersedes>`
- `tsq link remove <src> <dst> --type <relates_to|replies_to|duplicates|supersedes>`
- `tsq supersede <old-id> --with <new-id> [--reason <text>]`

Global options:
- `--json`
- `--exact-id`

Status alias:
- `done -> closed`

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
- atomic cache writes (`tasks.jsonl.tmp-*` -> rename)
- startup recovery ignores one malformed trailing JSONL line with warning
- deterministic rebuild from snapshot + event tail

## Locked V1 Decisions
1. Bun + TypeScript.
2. Root IDs fixed 6-char hash suffix with collision retry.
3. Child IDs append-only (`parent.n`), no gap reuse.
4. `relates_to` is bidirectional.
5. Keep generic `link add/remove` and canonical `supersede`.
6. `supersede` closes source and sets `superseded_by`; no dependency rewiring.
7. Claim policy strict CAS (no force-claim).
8. Canonical status is `closed`; alias `done`.
9. JSON envelope includes `schema_version: 1`.
10. Event IDs are ULID.
11. Reject dependency cycles; reject link self-edge.
12. Actor resolution: `TSQ_ACTOR` -> git `user.name` -> OS user -> `unknown`.

## Repo Conventions
- commit `.tasque/events.jsonl` and `.tasque/config.json`
- do not commit `.tasque/tasks.jsonl`
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
