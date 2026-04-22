## Overview
Implement verified codebase-improvement tasks from analysis and validator go/no-go pass. Goal: harden Tasque integrity checks, spec path safety, CLI parity, stale TUI cleanup, small perf hygiene, and stale docs.

## Constraints / Non-goals
- Keep edits surgical and aligned with existing Rust module boundaries.
- No broad write-pipeline abstraction; validators dropped that as too speculative.
- Keep enum codec centralization behavior-critical only; avoid macro/framework churn.
- `src/domain/projector_tasks.rs` must have a single owner because parent-cycle, spec metadata, and event-validation work all touch it.
- Cached-read full-log hashing is not in this implementation batch unless benchmark data justifies integrity tradeoff.

## Interfaces (CLI/API)
- `tsq list --label Foo` and `--label-any Foo,Bar` should normalize like label writes.
- `tsq init --wizard --yes --sync-branch <branch>` should preserve sync branch.
- `tsq spec check` must not read absolute or escaping spec paths from corrupted metadata.
- `tsq doctor` should report parent cycles.

## Data model / schema changes
No schema-version change planned. Existing event model stays append-only. Event read boundary should reject corrupt enum payloads as `EVENTS_CORRUPT`; projector should reject invalid in-memory events as `INVALID_EVENT`.

## Acceptance criteria
- Parent cycles are rejected during replay and surfaced by doctor on corrupt state.
- Spec metadata paths are canonical repo-local `.tasque/specs/<task-id>/spec.md`; unsafe metadata is invalid and not read.
- `task.updated.status` is no longer silently ignored.
- Label filter normalization matches label write normalization.
- Init wizard carries `sync_branch` through plan and resulting `InitInput`.
- Direct spec workflow tests cover missing spec, valid spec, drift, missing sections, and `--require-spec` claim behavior.
- Stale OpenTUI files are removed; typecheck still passes.
- Duplicate-title normalization uses static regexes.
- Stale docs/comments are refreshed without over-documenting internal APIs.

## Test plan
Per-task verification:
- `cargo test --test projector_invariants parent_cycle --quiet`
- `cargo test --test projector_invariants spec_attached --quiet`
- `cargo test --test event_read_validation --quiet`
- `cargo test --test list_search_parsing_parity label --quiet`
- `cargo test --test init_parity sync_branch --quiet`
- `cargo test --test spec_workflow --quiet`
- `cd tui-opentui && bun run typecheck`
- `cargo test duplicate --quiet`

Final gate:
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --quiet`
- `cd tui-opentui && bun run typecheck`
- `cargo build --release --locked`
- `install -m 0755 target/release/tsq ~/.local/bin/tsq`
- `tsq doctor --json`

## Parallel worker plan
- Core Integrity Owner: `tsq-8qwsmv5s.1`; write scope `src/domain/*`, `src/store/events.rs`, `src/app/storage.rs`, `src/app/repair.rs`, `src/app/service_query.rs`, integrity tests.
- CLI Filters Owner: `tsq-8qwsmv5s.4`; write scope `src/cli/parsers.rs`, `tests/list_search_parsing_parity.rs`.
- Init Owner: `tsq-8qwsmv5s.5`; write scope `src/cli/init_flow.rs`, maybe `src/cli/commands/meta.rs`, `tests/init_parity.rs`.
- Spec Workflow Owner: `tsq-8qwsmv5s.6`; write scope `tests/spec_workflow.rs`, minimal `tests/common/mod.rs`; starts after core spec contract.
- TUI Owner: `tsq-8qwsmv5s.7`; delete stale `tui-opentui/src/app.tsx`, `types.ts`, `view-model.ts`.
- Perf Hygiene Owner: `tsq-8qwsmv5s.8`; write scope `src/app/service_utils.rs`.
- Docs Owner: `tsq-8qwsmv5s.9`; write scope `CLAUDE.md`, `LEARNINGS.md`, docs front matter, merge-driver/sync rustdoc.
- Integrator: `tsq-8qwsmv5s.10`; blocked by all worker tasks; runs full gate and installs `tsq`.
