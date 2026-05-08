# Durability + Correctness Design

## Overview

This patch hardens Tasque's persistence boundary before more ergonomic and library-facing work continues. Tasque is local-first and JSONL-backed, so append/replay correctness matters more than internal cleanup. The most important bug class is a malformed trailing JSONL line that is ignored during read recovery but can corrupt the next append.

## Goals

- Make appending after a malformed trailing JSONL line safe.
- Validate events at the write boundary before appending.
- Validate state cache and snapshot invariants before accepting projected-state shortcuts.
- Add focused lock subsystem tests for multi-agent write safety.
- Fix documentation drift for `find --full`.

## Non-goals

- Do not redesign the event schema.
- Do not remove legacy `event_id` support.
- Do not introduce a database or backend abstraction.
- Do not implement batch-create transactions here.
- Do not change CLI behavior except clearer docs and stronger failure modes.

## Corrupt Tail Append Policy

Current read behavior tolerates one malformed trailing JSONL line and returns a warning. Append currently writes after whatever bytes are present. If the file ends with `{` and a later task append writes a valid event, replay can see one malformed final line made from both fragments and drop the new event.

Chosen policy: before appending, detect whether the final nonempty line is malformed JSON. If it is, truncate the file to the start of that line, then append new events. This makes the read recovery decision durable. Bytes already ignored by the reader are removed; earlier valid events remain untouched.

The append path should only trim the last malformed nonempty line. If any earlier line is malformed, appending must fail with the existing corrupt-events error rather than attempting repair.

## Event Write Validation

`append_events` should validate every record before serializing it. The write path and read path should share one event-record validator so code cannot append an event that replay later rejects.

- `id` or legacy `event_id` must be present and nonempty.
- `ts`, `actor`, `type`, and `task_id` must be present and nonempty.
- `payload` must be an object.
- Required event-specific payload string fields must be present and nonempty:
  - `task.created`: `title`
  - `task.status_set`: `status`
  - `task.noted`: `text`
  - `task.spec_attached`: `spec_path`, `spec_fingerprint`
  - `task.superseded`: `with`
  - `dep.added`, `dep.removed`: `blocker`
  - `link.added`, `link.removed`: `type`, `target`
- Optional typed payload fields must validate when present:
  - `priority` must be `0..=3`.
  - `labels` must be an array of strings.
  - `clear_description`, `clear_external_ref`, and `clear_discovered_from` must be booleans.
  - enum string fields must parse to known variants.
  - direct-reference string fields must be nonempty when present.

Legacy `event_id` remains accepted on write for now. A future library API can introduce a normalized event type with required `id`, but this patch should avoid schema churn.

## Projected-State Shortcut Validation

`read_state_cache` can currently deserialize cached state directly. Snapshot loading also accepts a projected `State` after metadata checks. Because cache and snapshot reads may bypass replay, projected-state shortcuts should validate state before returning it.

Minimum validation:

- Task ids are nonempty and match their map keys.
- Priority is in `0..=3`.
- Direct refs point to known tasks: `parent_id`, `superseded_by`, `duplicate_of`, `replies_to`, `discovered_from`.
- Parent chains do not cycle.
- Dependency and relation endpoints reference known tasks.
- `created_order` contains only known tasks and no duplicates.

If cache validation fails, ignore the cache and replay events. Do not fail the command solely because a rebuildable cache is invalid. If snapshot validation fails, skip that snapshot with the existing invalid-snapshot warning path, try older snapshots, and then fall back to full replay if no valid snapshot remains. Event replay remains the source of truth and can still fail if the event log itself is corrupt.

## Lock Tests

Add focused tests for `.tasque/.lock` behavior:

- Existing live lock times out with lock/concurrency error.
- Same-host dead-pid stale lock can be removed.
- Releasing a lock with the wrong owner does not delete another writer's lock and reports a lock ownership error.
- `with_write_lock` releases lock when callback returns error.

These tests cover the multi-agent write boundary where Tasque's local-first model depends on single-writer safety.

## Documentation Fix

Docs currently list `find --full` as if it works independently for every find mode. Implementation rejects `--full` without `--tree` for `find ready` and status-based `find`, while `find search --full` remains valid and prints full task details. Update:

- `README.md`
- `npm/README.md`
- `AGENTS-reference.md`
- `SKILLS/tasque/references/command-reference.md`

Required wording: for `find ready` and status-based `find` commands, `--full` is only valid with `--tree`; `find search --full` remains valid without `--tree`.

## Error Handling

- Corrupt non-final JSONL lines remain hard errors.
- Corrupt final JSONL line is trimmed only when appending new events.
- Invalid outbound events return validation/storage error before file write.
- Invalid outbound events leave `events.jsonl` byte-for-byte unchanged.
- Invalid cache or snapshot produces a shortcut miss and event replay, not user-visible failure unless replay fails.

## Testing

Required tests:

- A temp repo with valid event, malformed trailing `{`, append new event, delete `state.json`, then full replay includes valid original event and new event.
- Invalid outbound event cannot be appended.
- Invalid outbound append leaves `events.jsonl` byte-for-byte unchanged.
- Missing `link.*` target, missing `task.superseded` `with`, and invalid `priority` are rejected before append.
- Cache with invalid priority is ignored and replay produces valid state.
- Cache with bad refs is ignored and replay produces valid state.
- Invalid latest snapshot is ignored; older valid snapshot or full replay produces valid state.
- Lock behavior tests listed above.
- Existing CLI/release gates: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --quiet`.

## Acceptance Criteria

- No valid event appended after a malformed tail can be lost on replay.
- `append_events` refuses invalid event records before writing.
- Invalid state cache or snapshot cannot mask invalid task state.
- Lock subsystem has focused regression coverage.
- Docs accurately describe `find --full`.
- Existing behavior and JSON output shape remain compatible.
