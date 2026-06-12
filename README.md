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
tsq skills refresh          # update existing managed installs only
```

Skill install updates agent skill directories. Sync worktree setup belongs to
plain `tsq init`, `tsq migrate`, or explicit `--sync-branch`.

### Managed Skill Refresh

`tsq skills refresh` updates skill files in target directories that already have
a managed install. It does **not** create missing installs — use `tsq init --install-skill`
for first-time setup. Managed installs are identified by the `tsq-managed-skill:v1`
marker in `SKILL.md`. Directories containing this marker will be overwritten by
refresh. Remove the marker to opt out of automatic updates.

`tsq skills refresh` is repo-independent: it does not require `tsq init` or a
`.tasque/` directory.

The npm postinstall hook runs `tsq skills refresh` automatically after install.
It refreshes only existing managed `tasque` skill installs and does not create
missing ones. Refresh failures produce a warning but do not fail the install.
Set `TSQ_SKIP_SKILL_REFRESH=1` to skip postinstall refresh entirely.

## Command Reference

Canonical CLI and task contracts live in [`AGENTS-reference.md`](./AGENTS-reference.md).

Use built-in help for the freshest syntax:

```bash
tsq --help
tsq <command> --help
```

This README keeps install, quickstart, storage, and release guidance only, so command syntax is not maintained in multiple places.

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

`tsq plan <parent> --from plan.md` accepts the same bullet hierarchy plus
Markdown headings and inline labels:

```md
# CLI ergonomics #cli
  - [ ] Add strict root tests #test
```

```bash
tsq plan <parent-id> --from plan.md
tsq plan <parent-id> --from plan.md --apply
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
npm run test:postinstall --prefix npm
```

## CI

GitHub Actions CI (`.github/workflows/ci.yml`) runs:

1. `cargo fmt --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test --quiet`
4. `npm run test:postinstall --prefix npm`

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
