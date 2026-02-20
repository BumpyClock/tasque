# tasque

Local-first task tracker for coding agents.

- JSONL source of truth
- Repo-local storage in `.tasque/`
- No DB/service
- Durable restart + replay

## Quickstart

```bash
cargo run -- init --no-wizard
cargo run -- create "First task" --kind task -p 1
cargo run -- list
cargo run -- ready --json
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

## Storage Layout

Repo-local `.tasque/`:

- `events.jsonl`: canonical append-only event log
- `state.json`: derived projection cache (rebuildable, gitignored)
- `snapshots/`: periodic checkpoints (gitignored by default)
- `specs/<task-id>/spec.md`: canonical markdown specs attached to tasks
- `config.json`: config (`snapshot_every` default `200`)
- `.lock`: ephemeral write lock
- `.gitignore`: local-only artifacts (`state.json`, `.lock`, `snapshots/`, temp files)

Recommended commit policy:

- Commit `.tasque/events.jsonl` and `.tasque/config.json`
- Do not commit `.tasque/state.json`

## Command List

Global options:

- `--json`: JSON envelope output
- `--exact-id`: disable partial ID resolution

Commands:

- `tsq` (no args, TTY): open read-only TUI (List/Board views)
- `tsq init`
- `tsq init --install-skill|--uninstall-skill [--skill-targets ...]`
- `tsq create "Title" [--kind ...] [-p ...] [--parent <id>] [--external-ref <ref>] [--discovered-from <id>] [--planning <needs_planning|planned>] [--needs-planning] [--id <tsq-xxxxxxxx>] [--body-file <path|->]`
- `tsq show <id>`
- `tsq list [--status ...] [--assignee ...] [--external-ref <ref>] [--discovered-from <id>] [--kind ...] [--planning <needs_planning|planned>] [--dep-type <blocks|starts_after>] [--dep-direction <in|out|any>] [--tree]`
- `tsq ready [--lane <planning|coding>]`
- `tsq watch [--once] [--interval <seconds>] [--status <csv>] [--assignee <name>] [--tree]`
- `tsq tui [--once] [--interval <seconds>] [--status <csv>] [--assignee <name>] [--board]`
- `tsq stale [--days <n>] [--status <status>] [--assignee <name>] [--limit <n>]`
- `tsq doctor`
- `tsq update <id> [--title ...] [--status ...] [--priority ...] [--external-ref <ref>] [--clear-external-ref] [--discovered-from <id>] [--clear-discovered-from] [--planning <needs_planning|planned>]`
- `tsq update <id> --claim [--assignee <a>] [--require-spec]`
- `tsq orphans`
- `tsq spec attach <id> [source] [--file <path> | --stdin | --text <markdown>]`
- `tsq spec check <id>`
- `tsq dep add <child> <blocker> [--type <blocks|starts_after>]`
- `tsq dep tree <id> [--direction <up|down|both>] [--depth <n>]`
- `tsq dep remove <child> <blocker> [--type <blocks|starts_after>]`
- `tsq link add <src> <dst> --type <relates_to|replies_to|duplicates|supersedes>`
- `tsq link remove <src> <dst> --type <relates_to|replies_to|duplicates|supersedes>`
- `tsq duplicate <id> --of <canonical-id> [--reason <text>]`
- `tsq duplicates [--limit <n>]`
- `tsq merge <source-id...> --into <target-id> [--reason <text>] [--force] [--dry-run]`
- `tsq supersede <old-id> --with <new-id> [--reason <text>]`
- `tsq note add <id> <text>`
- `tsq note list <id>`
- `tsq label add <id> <label>`
- `tsq label remove <id> <label>`
- `tsq label list`
- `tsq history <id> [--limit <n>] [--type <event-type>] [--actor <name>] [--since <iso>]`
