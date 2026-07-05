# Fix git-backed sync correctness

## Goal
Make Tasque git-backed sync safe for local-first multi-machine use.

## Approved scope
- New canonical task IDs become collision-resistant flat random IDs using existing accepted `tsq-<8 crockford>` shape.
- Existing sequential and legacy child IDs remain readable.
- Alias selection becomes ambiguity-safe.
- Sync becomes commit/fetch/merge/replay-validate/push with lock and retry.
- Conflicts become structured/actionable and block writes until resolved.
- Specs conflicts remain manual guided conflicts.

## Non-goals
- Rewriting old IDs.
- Automatic remap of already-collided repos.
- Event-sourced specs.
- `merge=union` for specs.