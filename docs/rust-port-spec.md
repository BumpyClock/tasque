# Rust Port Spec

## Overview
Port the Bun/TypeScript CLI in `src/` to a Rust implementation that preserves 1:1 runtime behavior, storage formats, and CLI output. The new Rust source must live under `src-rust/`. The Rust code should be idiomatic (Rust-first design), while still matching the existing CLI contract and data model.

## Constraints / Non-goals
- Preserve functional parity and data compatibility with existing `.tasque/` files and JSONL events.
- Do not change CLI flags, command names, output formats, or error codes/messages.
- No architecture parity requirement with TS; Rust structure can differ if behavior matches.
- No new features beyond parity and safety fixes required by the port.

## Interfaces (CLI/API)
- CLI: `tsq` with commands and flags as documented in README and current CLI help.
- Global flags: `--json`, `--exact-id`.
- Environment variables: `TSQ_ACTOR`, `TSQ_LOCK_TIMEOUT_MS`, `TSQ_SKILLS_DIR`, `CODEX_HOME` (and implicit OS user lookup).
- Data files: `.tasque/events.jsonl`, `.tasque/state.json`, `.tasque/snapshots/`, `.tasque/config.json`, `.tasque/.lock`, `.tasque/specs/<id>/spec.md`.

## Data model / schema changes
- No schema changes. Use the same event types, task fields, and envelope format (`schema_version=1`).
- Maintain backward-compatible event parsing for legacy `event_id` field alias.
- Preserve snapshot file naming and pruning behavior.

## Acceptance criteria
- All CLI commands behave the same as the Bun implementation:
  - Create/update/claim/close/reopen/duplicate/supersede/merge/dep/link/label/note/spec/history/list/ready/stale/watch/doctor/repair/orphans/init.
  - Error codes/messages and exit codes match for validation, IO, and lock errors.
  - JSON envelope output matches schema and command path.
  - Human output content matches current strings (including tree/watch rendering and headings).
- Storage layer preserves append-only JSONL events, rebuildable cache, lock semantics, and snapshot retention.
- Spec attach/check and claim gating behave identically.
- Dependency semantics (blocks vs starts_after) and ready detection match TS behavior.
- Rust CLI can operate on existing `.tasque/` directories produced by the Bun version.

## Test plan
- Create a Rust test suite that mirrors existing TS tests for core behavior:
  - Store events parsing/validation, locks, snapshots.
  - Projector and domain validation.
  - CLI behavior for JSON envelopes, error codes, and key outputs.
- Run the Rust tests and a small manual smoke test of CLI commands against a temp repo.
