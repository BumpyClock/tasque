# Task sync across machines

Tasque is local-first. `tsq sync` is the only path that touches the network,
and only when a remote is configured. It turns the local `.tasque/` data into a
git branch you can share between machines and collaborators.

## Where task data lives

In a git repo, `tsq init` moves task data off your code branch into a dedicated
**sync worktree** on a separate branch (default `tsq-sync`, configurable via
`--sync-branch` / `--worktree-name`). Your code branch keeps only
`.tasque/config.json` so Tasque can find the sync branch.

- `.tasque/events.jsonl` — append-only event log (canonical source of truth).
- `.tasque/specs/<task-id>/spec.md` — task specs.
- `.tasque/config.json` — project settings (in the main worktree).
- `.tasque/state.json`, `.tasque/.lock`, `.tasque/snapshots/` — local-only,
  gitignored, always rebuildable.

## What `tsq sync` does

`tsq sync` runs a local-first two-way sync in this order:

1. **Commit** local changes to the sync branch (events, specs, config).
2. **Fetch** the remote branch (upstream first, then `origin`), if it exists.
3. **Merge** fetched changes. `events.jsonl` is merged by the
   `tasque-events` driver (see below); other files use git's default merge.
4. **Push** the result back to the remote, setting upstream on first push.

`tsq sync --no-push` stops after step 1: local commit only, no network. The
bundled pre-push hook runs exactly this so a manual `git push` from the sync
worktree still commits pending task updates first.

When no remote/remote branch exists, `tsq sync` commits locally and reports that
push was skipped.

## How conflicts resolve

Different files merge differently by design:

- `events.jsonl`: the `tasque-events` driver unions events by event id,
  deduplicates identical events, and replay-validates the result. Independent
  edits auto-merge. Same event id with divergent payloads, or an invalid merged
  history, surfaces a structured `MERGE_*` error.
- `specs/<id>/spec.md`: git default 3-way text merge. Conflicting spec edits
  leave conflict markers and require manual resolution.
- `config.json`: git default 3-way text merge. If both sides edit config,
  resolve it manually.

The event merge is commutative for independent task creates: pulling A then B
yields the same `events.jsonl` as B then A, because events are deduplicated by
id (ULID `id`, legacy `event_id` accepted on read).

### Conflict UX

If a git merge conflict cannot be auto-resolved, `tsq sync` leaves the merge in
progress and returns a structured error with the conflicted paths and the sync
worktree path (`SYNC_MERGE_CONFLICT`). Resolve the conflicts in that worktree,
then re-run `tsq sync`: with no unmerged paths remaining it completes the merge
and pushes; if unmerged paths still exist it re-emits the conflict.

Spec conflicts in particular must be resolved by a human: open the marked
`spec.md`, pick the intended content, remove the `<<<<<<<` / `=======` / `>>>>>>>`
markers, save, then re-run `tsq sync`.

## Task IDs

New tasks get a flat random canonical id of the form `tsq-<8 lowercase crockford
chars>` (for example `tsq-7k3q9x2a`). Random ids avoid collisions when two
machines create tasks independently and later sync.

Existing ids keep working unchanged:

- Sequential root ids `tsq-<number>` (for example `tsq-42`) — still valid.
- Child ids `<parent>.<n>` (for example `tsq-42.3`) — still valid.
- Legacy 8-char ids `tsq-<8 crockford>` — still valid.

`--id <id>` on `tsq create` accepts any of these shapes. Commands that take a
task id accept the id, exact alias, or a unique alias prefix. An exact alias that
matches more than one task returns `TASK_ID_AMBIGUOUS` rather than guessing.

## Commit policy

For the **sync worktree** (task data), commit:

- `.tasque/events.jsonl`
- `.tasque/specs/` (specs are meant to sync)
- `.tasque/config.json`

Do not commit `.tasque/state.json`, `.tasque/.lock`, or `.tasque/snapshots/` —
they are gitignored and rebuildable. `tsq sync` handles this set automatically;
you should not stage task data manually.

For the **main code worktree**, commit:

- `.tasque/config.json` (the pointer to the sync branch)
- `.gitattributes` (it registers `.tasque/events.jsonl merge=tasque-events`)

`.gitattributes` matters: without the `tasque-events` driver entry, event merges
fall back to git's text merge and can corrupt the log. `tsq init` /
`tsq migrate` ensure the entry exists; keep it committed.
