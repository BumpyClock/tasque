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

## Command Reference

Canonical CLI and task contracts live in the repository [`AGENTS-reference.md`](../AGENTS-reference.md).

For installed packages, use command help for current syntax:

```bash
tsq --help
tsq <command> --help
```

This npm README keeps install and packaging guidance only, so command syntax is not maintained in multiple places.

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

Release and npm packages also include a bundled OpenTUI executable:

- Linux/macOS: `tsq-tui`
- Windows: `tsq-tui.exe`

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
  - Bundles the OpenTUI frontend as a standalone executable next to `tsq`
  - Uploads release artifacts + checksums
- `npm-publish` (`.github/workflows/npm-publish.yml`)
  - On published GitHub release, builds platform npm packages
  - Packages `tsq` and `tsq-tui` together for each platform
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
