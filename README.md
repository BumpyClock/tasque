# tasque

Local-first task tracker for coding agents.

- JSONL source of truth
- Git worktree-backed `.tasque/` storage by default in git repos
- No DB/service
- Durable restart + replay

## Install

```bash
npm install -g @bumpyclock/tasque
tsq --version
```

From source:

```bash
cargo build --release
target/release/tsq --version
```

## Quickstart

```bash
tsq init --no-wizard
tsq create "First task" --kind task -p 1
tsq find open
tsq find ready --lane coding --format json
```

Install or refresh the bundled agent skill:

```bash
tsq init --install-skill --force-skill-overwrite
```

Skill install updates agent skill directories. Sync worktree setup belongs to
plain `tsq init`, `tsq migrate`, or explicit `--sync-branch`.

## Command List

Global options:

- `--format human|json`: output format (`human` default)
- `--json`: shorthand for `--format json`
- `--exact-id`: disable partial ID resolution

Commands:

- `tsq` (no args, TTY): open read-only TUI (List/Board views)
- `tsq init [--wizard|--no-wizard] [--yes] [--preset <name>] [--sync-branch|--worktree-name <name>]`
- `tsq init --install-skill|--uninstall-skill [--skill-targets ...] [--skill-name <name>] [--force-skill-overwrite]`
- `tsq create <title...> [--kind ...] [-p ...] [--parent <id>] [--from-file tasks.md] [--description <text>] [--external-ref <ref>] [--discovered-from <id>] [--planned|--needs-plan] [--ensure] [--id <tsq-xxxxxxxx>] [--body-file <path|->]`
- `tsq show <id> [--with-spec]`
- `tsq find ready [--lane <planning|coding>] [--assignee <name>] [--unassigned] [--kind ...] [--label ...] [--planning <needs_planning|planned>] [--tree [--full]]`
- `tsq find <blocked|open|in-progress|deferred|done|canceled> [filters...] [--tree [--full]]`
- `tsq find search <query> [--full]`

Note: for `find ready` and status-based `find` commands, `--full` is only valid with `--tree`. `--tree --full` keeps the full status set instead of applying the default tree status narrowing. `find search --full` remains valid without `--tree`.
- `tsq watch [--once] [--interval <seconds>] [--status <csv>] [--assignee <name>] [--tree] [--flat]`

`watch` renders the task tree by default for human output. Use `--flat` for the compact list view.
- `tsq tui [--once] [--interval <seconds>] [--status <csv>] [--assignee <name>] [--board|--epics]`
- `tsq stale [--days <n>] [--status <status>] [--assignee <name>] [--limit <n>]`
- `tsq doctor`
- `tsq repair [--fix] [--force-unlock]`
- `tsq edit <id> [--title ...] [--description ...] [--clear-description] [--priority ...] [--external-ref <ref>] [--clear-external-ref] [--discovered-from <id>] [--clear-discovered-from]`
- `tsq claim <id> [--assignee <a>] [--start] [--require-spec]`
- `tsq assign <id> --assignee <a>`
- `tsq start <id>`
- `tsq planned <id>`
- `tsq needs-plan <id>`
- `tsq open <id>`
- `tsq blocked <id>`
- `tsq defer <id> [--note <text>]`
- `tsq done <id...> [--note <text>]`
- `tsq reopen <id...> [--note <text>]`
- `tsq cancel <id...> [--note <text>]`
- `tsq orphans`
- `tsq spec <id> [--file <path> | --stdin | --text <markdown> | --show | --check] [--force]`
- `tsq block <task> by <blocker>`
- `tsq unblock <task> by <blocker>`
- `tsq order <later> after <earlier>`
- `tsq unorder <later> after <earlier>`
- `tsq deps <id> [--direction <up|down|both>] [--depth <n>]`
- `tsq relate <src> <dst>`
- `tsq unrelate <src> <dst>`
- `tsq duplicate <id> of <canonical-id> [--note <text>]`
- `tsq duplicates [--limit <n>]`
- `tsq merge <source-id...> --into <target-id> [--reason <text>] [--force] [--dry-run]`
- `tsq supersede <old-id> with <new-id> [--note <text>]`
- `tsq note <id> <text>`
- `tsq note <id> --stdin`
- `tsq notes <id>`
- `tsq label <id> <label>`
- `tsq unlabel <id> <label>`
- `tsq labels`
- `tsq history <id> [--limit <n>] [--type <event-type>] [--actor <name>] [--since <iso>]`
- `tsq sync [--no-push]`
- `tsq hooks install [--force]`
- `tsq hooks uninstall`
- `tsq migrate [--sync-branch|--worktree-name <name>]`
- `tsq merge-driver <ancestor> <ours> <theirs>`

Git repos default to worktree mode: `tsq init` creates/configures the `tsq-sync`
branch and stores task data in a dedicated git worktree. Use `--sync-branch <name>`
or `--worktree-name <name>` to choose a different branch/worktree name. Existing git repos with main-tree `.tasque`
data and no `sync_branch` migrate automatically on the next `tsq` command. Fresh clones fetch
the configured sync branch and create the worktree on first use. `tsq sync` pushes
the sync branch to `origin` and sets upstream automatically when needed.
Non-git directories use local `.tasque/` storage.

## `tasks.md` Batch Format

Use `tasks.md` when a plan naturally reads as a checklist. Each bullet creates
one task. Two-space indentation creates parent/child hierarchy. Checkbox bullets
are accepted. Tabs, odd indentation, indentation jumps, and indented first
bullets are rejected with line-numbered validation errors.

```md
- Parent task
  - Child task
    - Grandchild task
- [ ] Another parent task
```

```bash
tsq create --from-file tasks.md
tsq create --parent <id> --from-file tasks.md
tsq create --from-file tasks.md --ensure
```

## Version

```bash
cargo run -- --version
```

## Build

```bash
cargo build --release
```

Binary output:

- Linux/macOS: `target/release/tsq`
- Windows: `target/release/tsq.exe`

## Test + Lint + Format

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --quiet
```

## CI

GitHub Actions CI (`.github/workflows/ci.yml`) runs:

1. `cargo fmt --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test --quiet`

All steps must pass before merging.

## Release Workflows

- `Release Please` (`.github/workflows/release-please.yml`)
  - Runs Rust quality checks
  - Opens/updates release PRs using Rust release type
- `Release From Cargo` (`.github/workflows/release-from-package.yml`)
  - Manual release creation from `Cargo.toml` version
  - Optional `version` input must match Cargo version
- `Release` (`.github/workflows/release.yml`)
  - On published GitHub release, builds matrix binaries (Linux/macOS/Windows)
  - Uploads release artifacts + checksums
- `npm-publish` (`.github/workflows/npm-publish.yml`)
  - On published GitHub release, builds platform npm packages
  - Publishes platform packages, then `@bumpyclock/tasque`

## Storage Layout

Git repos default to a dedicated sync worktree:

- `tsq init` configures `tsq-sync` by default and redirects data operations there.
- Fresh clones fetch the configured sync branch and create the worktree on first use.
- `tsq sync` pushes the sync branch to `origin` and sets upstream automatically when needed.
- Existing git repos with main-tree `.tasque` data migrate automatically when `tsq`
  next resolves the project root.
- The main worktree keeps `.tasque/config.json` so `tsq` can find the sync branch.
- The sync worktree owns the canonical `.tasque/events.jsonl`, specs, snapshots, and cache.

Non-git directories use repo-local `.tasque/`:

- `events.jsonl`: canonical append-only event log
- `state.json`: derived projection cache (rebuildable, gitignored)
- `snapshots/`: periodic checkpoints (gitignored by default)
- `specs/<task-id>/spec.md`: canonical markdown specs attached to tasks
- `config.json`: config (`snapshot_every` default `200`)
- `.lock`: ephemeral write lock
- `.gitignore`: local-only artifacts (`state.json`, `.lock`, `snapshots/`, temp files)
- `tasks.jsonl`: legacy state-cache name; read-only fallback when `state.json` is absent, removal target

Recommended commit policy:

- Commit `.tasque/events.jsonl` and `.tasque/config.json`
- Do not commit `.tasque/state.json`
- Do not create or edit `.tasque/tasks.jsonl`
