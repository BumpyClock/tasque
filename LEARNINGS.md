# LEARNINGS

## Architecture
- JSONL append-only events (`.tasque/events.jsonl`) are the sole source of truth. Derived state (`.tasque/state.json`) is a rebuildable cache — never edit it manually.
- Single-machine write lock via `.tasque/.lock` (`open wx`, short retry). No multi-writer guarantees.
- Snapshot loader scans newest-to-oldest valid snapshot; writes prune to latest 5 files.
- Event IDs are ULIDs. Canonical event field is `id` with legacy `event_id` alias accepted on read. Task IDs are 8-char Crockford base32 random (root) or `<parent>.<n>` (children, append-only).
- TasqueService is split into focused modules: `service.ts` (facade, 454 LOC), `service-types.ts`, `service-utils.ts`, `service-lifecycle.ts` (mutations), `service-query.ts` (queries). Internal modules receive a `ServiceContext` object.
- Status transitions emit `task.status_set` events; non-status field updates emit `task.updated`. Supersede/duplicate emit both.
- Shared helpers: `src/domain/events.ts` (event factory), `src/cli/terminal.ts` (width/density), `buildDependentsByBlocker` lives in `dep-tree.ts`.
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
- Query tokenizer must handle `field:\"quoted value\"` as one token; quote handling only at token-start breaks field-prefixed quoted filters.
- `withWriteLock` throws `AggregateError` when both callback and release fail; uses try/catch flow (not `finally`) to satisfy biome `noUnsafeFinally`.
- Stale lock cleanup uses atomic `rename` + content verification to prevent TOCTOU races between concurrent cleaners.
- Projector validates dep/link targets with `requireTask()` — repair tests must inject orphans via state cache, not raw events, to bypass this validation.
- `repair --fix` computes plan inside write lock using locked snapshot to prevent plan/apply drift.
- `process.exitCode` is sticky within a long-lived process. Reset at command entry (`preAction`) so a prior failing command does not cause later successful commands to exit non-zero.

## Build & Release
- `bun run build` compiles a single binary; `bun run release` emits a platform artifact + `SHA256SUMS.txt` in `dist/releases/`.
- `tsq init` skill lifecycle for agents uses managed-marker semantics: install, uninstall, idempotent update, non-managed skip unless `--force`.

## Recent Updates
- 2026-02-18: Event parsing now uses Zod at JSONL boundaries; canonical event field is `id` while reads still accept legacy `event_id`.
- 2026-02-18: Cache path migrated to `.tasque/state.json` with backward-compatible reads from legacy `.tasque/tasks.jsonl`.
