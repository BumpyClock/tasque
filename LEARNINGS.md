# LEARNINGS

## Architecture
- JSONL append-only events (`.tasque/events.jsonl`) are the sole source of truth. Derived state (`.tasque/tasks.jsonl`) is a rebuildable cache — never edit it manually.
- Single-machine write lock via `.tasque/.lock` (`open wx`, short retry). No multi-writer guarantees.
- Snapshot loader scans newest-to-oldest valid snapshot; writes prune to latest 5 files.
- Event IDs are ULIDs. Task IDs are 6-char hashes (root) or `<parent>.<n>` (children, append-only).
- JSON output uses a universal envelope with `schema_version=1`.

## Design Decisions
- Bidirectional `relates_to` links; canonical `supersede` closes the source task and sets `superseded_by` without dependency/link rewiring.
- `duplicate` closes source, adds `duplicate_of` metadata — no dependency rewiring (same pattern as supersede).
- Strict CAS (compare-and-swap) claim semantics.
- Spec required sections: `Overview`, `Constraints / Non-goals`, `Interfaces (CLI/API)`, `Data model / schema changes`, `Acceptance criteria`, `Test plan`.
- Timestamp filters (`--created-after`, `--updated-after`, `--closed-after`) require strict ISO timestamps; reject natural-language dates.

## Pitfalls
- `resolveTaskId` throws `TASK_NOT_FOUND`, not `NOT_FOUND`.
- Negated search queries (`-field:value`) need a `--` separator before them due to commander treating leading `-` as option flags.
- Commander option conflict detection (e.g. `--assignee` vs `--unassigned`) must use option events, not just value checks, to handle both `--flag value` and `--flag=value` forms consistently.
- Lock contention timeout (`LOCK_TIMEOUT`) is a concurrency-class failure and must keep `exitCode: 3` (not IO/storage code 2).

## Build & Release
- `bun run build` compiles a single binary; `bun run release` emits a platform artifact + `SHA256SUMS.txt` in `dist/releases/`.
- `tsq init` skill lifecycle for agents uses managed-marker semantics: install, uninstall, idempotent update, non-managed skip unless `--force`.
