# Library Facade + Simplification Design

## Overview

Tasque remains a CLI-first crate today, but it should have a deliberate future library entry point for reuse by another project. The current crate exposes many internals through `lib.rs`; that is useful for tests but too broad to become a stable API. This design adds a small facade while keeping CLI behavior unchanged and leaves deep internals private in spirit until visibility can be tightened safely.

## Goals

- Add `tasque::api::TasqueClient` as the intended reusable entry point.
- Keep CLI behavior and command output unchanged.
- Mark current internal modules as unstable/internal rather than documenting them as stable API.
- Move batch create toward one transaction.
- Reduce clone-heavy query/list/tree internals where straightforward.
- Centralize enum string codecs.
- Include the quick OpenTUI tree complexity fix.

## Non-goals

- Do not promise semver-stable library API yet.
- Do not add a backend/plugin abstraction.
- Do not redesign storage.
- Do not implement a long-lived OpenTUI protocol.
- Do not split every oversized module as part of this work.
- Do not remove legacy compatibility paths.

## Public Facade

Add a new `api` module with a `TasqueClient` wrapper. The wrapper owns or contains the current `TasqueService`, but callers should not need to depend on `app`, `store`, `cli`, projection, or sync modules.

Initial shape:

```rust
pub mod api;

pub struct TasqueClient {
    service: TasqueService,
}
```

Initial methods should cover basic task flows:

- construct from repo root, actor, and clock, or from default runtime context
- `create`
- `show`
- `list`
- `find_ready`
- `note_add`
- `spec_content`

The facade can reuse selected DTOs at first, but each exported DTO should be explicitly chosen. Avoid exporting storage, git, render, parser, projection, or CLI types through `api`.

## Visibility Policy

For now, `lib.rs` can keep existing modules public if tests and binaries rely on that shape. The docs and rustdoc should state that only `api` is intended for external use. Later cleanup can reduce visibility module by module.

Recommended policy:

- `api`: intended public surface.
- `types`: selected stable data types may be re-exported by `api`.
- `errors::TsqError`: public enough for facade results.
- `app`, `store`, `domain`, `cli`, `skills`, `output`: internal and unstable unless re-exported through `api`.

## Batch Create Transaction

`create --from-file` currently loops over parsed bullets and calls `service.create` once per bullet. That repeats lock acquisition, state loading, event append, projection persistence, and sync-worktree auto-commit.

Add a service/API batch path that:

- takes parsed or structured create requests
- acquires one write lock
- loads state once
- resolves parents as each item is projected
- appends all events once
- persists projection once
- preserves existing JSON response shape for CLI

This should make `tasks.md` import atomic: either the batch succeeds or no partial task list is written.

## Query/List/Tree Simplification

Internals should move toward borrowed filtering and sorting:

- Filter `&Task` references where possible.
- Sort borrowed task refs internally.
- Clone only final return payloads.

This keeps external behavior the same while reducing allocation pressure in `list`, `search`, and `list_tree`.

Do not over-engineer indexes yet. Prefix lookup, ready indexes, and search caches can wait until task counts or profiling justify them.

## Enum String Codecs

String conversions are duplicated across renderers and services. Centralize stable string helpers for:

- task status
- task kind
- planning state
- dependency type
- relation type
- event type
- dependency direction

Preferred location: a domain or formatting-neutral module, not CLI render code. CLI renderers can still apply styling after getting canonical strings.

## TUI Quick Fix

OpenTUI `buildTreeLines` has an `O(R^2)` root loop because it calls `roots.indexOf(root)` inside a root iteration. Replace with indexed iteration.

This is a low-risk fix and does not require a TUI protocol redesign.

## Out-of-Scope Strategic Work

OpenTUI currently refreshes by spawning `tsq --json list`. A long-lived JSON frame stream or shared Rust frame endpoint could improve responsiveness, but it adds protocol surface. Defer until after the facade exists and the CLI durability work is complete.

Large module splitting is also deferred unless a touched file crosses a clarity threshold during implementation.

## Documentation

Add concise docs that explain:

- Tasque is CLI-first.
- `tasque::api::TasqueClient` is the only intended library entry point.
- Other public modules are internal and unstable for now.
- Library API stability is provisional until a downstream user exists.

Avoid documenting every current `pub` item as stable. That would freeze accidental internals.

## Testing

Required tests:

- A small compile-time/integration test exercising `api::TasqueClient` create/list/show flow.
- CLI tests proving `create --from-file` JSON output shape is unchanged.
- Batch-create failure regression proves no partial write when a later item fails.
- Existing CLI/release gates: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --quiet`.

Optional:

- Add a tiny example crate or doc test once the facade stabilizes.

## Acceptance Criteria

- External users can start from `tasque::api::TasqueClient` without touching internals.
- CLI behavior and JSON contracts remain unchanged.
- Batch create writes atomically and keeps existing response shape.
- Query/list/tree code clones less without changing order or filters.
- Enum string conversions have one canonical implementation.
- OpenTUI root tree loop is linear.
- Internal modules are documented as unstable, not as future stable API.
