# tasque

Local-first task tracker for coding agents.

- JSONL source of truth.
- Repo-local storage in `.tasque/`.
- No DB/service.
- Durable restart + replay.

## Quickstart

```bash
bun install
bun run src/main.ts init
bun run src/main.ts create "First task" --kind task -p 1
bun run src/main.ts list
bun run src/main.ts ready --json
```

`package.json` also exposes `tsq` as bin entry. In local dev, `bun run src/main.ts ...` is the simplest path.

## Version

```bash
tsq -V
tsq --version
```

Both print the current version from `package.json`.

## Build And Release

```bash
bun run build
bun run release
bun run version:bump -- --bump patch
```

- `build` compiles single-file binary to `dist/tsq` (or `dist/tsq.exe` on Windows).
- `release` rebuilds, writes platform artifact, generates task-derived release notes, then writes checksums under `dist/releases/`.
- `version:bump` updates release versions in tracked files:
  - package version: `bun run version:bump -- --bump patch` or `bun run version:bump -- --version 1.3.0`
  - schema version (when needed): `bun run version:bump -- --schema 2`
- release artifacts:
  - `tsq-v<version>-<platform>-<arch>[.exe]`
  - `RELEASE_NOTES.md`
  - `RELEASE_NOTES.json`
  - `SHA256SUMS.txt` (contains checksums for all artifacts above)
- release notes baseline uses latest git tag timestamp; with no tag, all closed tasks are considered.
- set `TSQ_BIN` to override which installed `tsq` executable release hooks call.

## CI

Pull requests and pushes to `main` run the `check` job via GitHub Actions (`.github/workflows/ci.yml`).

The job executes `bun run doctor`, which runs:

1. `typecheck` — TypeScript strict checks
2. `fmt` — Biome formatting
3. `lint` — Biome linting
4. `build` — compile binary
5. `test` — full test suite

All steps must pass before merging.

## Releasing

Releases are automated via [release-please](https://github.com/googleapis/release-please).

### How it works

1. Merge PRs with [Conventional Commits](https://www.conventionalcommits.org/) messages.
2. `release-please` opens a version-bump PR updating `package.json` and `CHANGELOG.md`.
3. Merge the release PR — a GitHub Release is created automatically.
4. The release workflow builds platform binaries (Linux, macOS, Windows) and uploads them with checksums.

You can also trigger the release-planning workflow manually:

1. Go to GitHub Actions.
2. Run `Release Please` (`.github/workflows/release-please.yml`) via **Run workflow**.
3. Merge or update the generated release PR.

Detailed runbook: `docs/releasing.md`.

Seamless manual release from `package.json`:

1. Ensure `package.json` contains the target version.
2. Run `Release From Package` (`.github/workflows/release-from-package.yml`) from GitHub Actions.
3. Optional: set workflow input `version` to enforce an exact expected value.
4. The workflow validates package version sync and publishes release tag `v<package.version>`.
5. Publish then triggers `Release` workflow, which verifies tag/version sync again before uploading artifacts.

### Rollback

If a release is broken:

1. **Delete the GitHub Release** — removes download links immediately.
2. **Delete the tag** — `git push --delete origin v<version>`.
3. **Revert the commit** — `git revert <sha>` on `main`.
4. **Merge revert** — release-please will handle the next version bump.
5. **Verify** — confirm the bad version is no longer available.

## Storage Layout

Repo-local `.tasque/`:

- `events.jsonl`: canonical append-only event log.
- `state.json`: derived projection cache (rebuildable, gitignored).
- `snapshots/`: periodic checkpoints (gitignored by default).
- `specs/<task-id>/spec.md`: canonical markdown specs attached to tasks.
- `config.json`: config (`snapshot_every` default `200`).
- `.lock`: ephemeral write lock.
- `.gitignore`: local-only artifacts (`state.json`, `.lock`, `snapshots/`, temp files).

Recommended commit policy:

- Commit `.tasque/events.jsonl` and `.tasque/config.json`.
- Do not commit `.tasque/state.json`.

## Planning Workflow

Tasque tracks lifecycle and planning separately:

- `status`: `open|in_progress|blocked|deferred|closed|canceled`
- `planning_state`: `needs_planning|planned`

Typical loop:

1. `tsq ready --lane planning`
2. Collaborate on scope/spec, then `tsq update <id> --planning planned`
3. `tsq ready --lane coding`
4. Implement and close with `tsq update <id> --status closed`

See `docs/planning-workflow.md` for detailed examples.

## Command List

Global options:

- `--json`: JSON envelope output.
- `--exact-id`: disable partial ID resolution.

Commands:

- `tsq init [--wizard|--no-wizard] [--yes] [--preset <minimal|standard|full>] [--install-skill|--uninstall-skill] [--skill-targets <csv>] [--skill-name <name>] [--force-skill-overwrite] [--skill-dir-claude <path>] [--skill-dir-codex <path>] [--skill-dir-copilot <path>] [--skill-dir-opencode <path>]`
- `tsq create "<title>" [--kind task|feature|epic] [-p|--priority 0..3] [--parent <id>] [--description <text>] [--body-file <path|->] [--external-ref <ref>] [--discovered-from <id>] [--planning <needs_planning|planned>] [--needs-planning] [--id <tsq-xxxxxxxx>]`
- `tsq show <id>`
- `tsq list [--status <open|in_progress|blocked|deferred|closed|canceled|done>] [--assignee <name>] [--external-ref <ref>] [--discovered-from <id>] [--kind <task|feature|epic>] [--label <label>] [--label-any <csv-or-repeat>] [--created-after <iso>] [--updated-after <iso>] [--closed-after <iso>] [--unassigned] [--id <csv-or-repeat>] [--planning <needs_planning|planned>] [--dep-type <blocks|starts_after>] [--dep-direction <in|out|any>] [--tree] [--full]`
- `tsq ready [--lane <planning|coding>]`
- `tsq watch [--once] [--interval <seconds>] [--status <csv>] [--assignee <name>] [--tree]`
- `tsq stale [--days <n>] [--status <open|in_progress|blocked|deferred|closed|canceled|done>] [--assignee <name>] [--limit <n>]`
- `tsq doctor`
- `tsq update <id> [--title <text>] [--status <...>] [--priority <0..3>] [--description <text>] [--clear-description] [--external-ref <ref>] [--clear-external-ref] [--discovered-from <id>] [--clear-discovered-from] [--planning <needs_planning|planned>]`
- `tsq update <id> --claim [--assignee <name>] [--require-spec]`
- `tsq orphans`
- `tsq note add <id> <text>`
- `tsq note list <id>`
- `tsq spec attach <id> [source] [--file <path> | --stdin | --text <markdown>]`
- `tsq spec check <id>`
- `tsq dep add <child> <blocker> [--type <blocks|starts_after>]`
- `tsq dep tree <id> [--direction <up|down|both>] [--depth <n>]`
- `tsq dep remove <child> <blocker> [--type <blocks|starts_after>]`
- `tsq link add <src> <dst> --type <relates_to|replies_to|duplicates|supersedes>`
- `tsq link remove <src> <dst> --type <relates_to|replies_to|duplicates|supersedes>`
- `tsq duplicate <id> --of <canonical-id> [--reason <text>]`
- `tsq duplicates [--limit <n>]` (dry-run scaffold)
- `tsq merge <source-id...> --into <target-id> [--reason <text>] [--force] [--dry-run]`
- `tsq supersede <old-id> --with <new-id> [--reason <text>]`
- `tsq label add <id> <label>`
- `tsq label remove <id> <label>`
- `tsq label list`
- `tsq history <id> [--limit <n>] [--type <event-type>] [--actor <name>] [--since <iso>]`

Tree mode notes:

- `--tree` defaults to showing only `open` and `in_progress` tasks.
- Use `--tree --full` to include all statuses in tree output.
- `>=120` columns: metadata stays inline with title.
- `90-119` columns: metadata moves to a second line when needed.
- `<90` columns: metadata always uses a second line, and long titles are truncated.

Skill installer via `init`:

- `tsq init --install-skill`
- `tsq init --uninstall-skill`
- Explicit `--install-skill` / `--uninstall-skill` bypasses auto-wizard mode by default. Use `--wizard` if you want interactive prompts.
- `--skill-targets claude,codex,copilot,opencode|all` (default `all`)
- `--skill-name <name>` (default `tasque`)
- `--force-skill-overwrite` (overwrite unmanaged skill dirs)
- target dir overrides:
  - `--skill-dir-claude <path>`
  - `--skill-dir-codex <path>`
  - `--skill-dir-copilot <path>`
  - `--skill-dir-opencode <path>`

Default target roots:

- Claude: `~/.claude/skills`
- Codex: `${CODEX_HOME:-~/.codex}/skills`
- Copilot: `~/.copilot/skills`
- OpenCode: `~/.opencode/skills`

Managed skill source:

- Install/update copies the full directory tree from `SKILLS/<skill-name>/` into each target root.
- Include `SKILL.md` with marker `tsq-managed-skill:v1`; installer uses markers in both `SKILL.md` and `README.md` to detect managed directories.

## JSON Envelope

All commands with `--json` return:

```json
{
  "schema_version": 1,
  "command": "tsq ready",
  "ok": true,
  "data": {
    "tasks": []
  }
}
```

Error shape:

```json
{
  "schema_version": 1,
  "command": "tsq dep add",
  "ok": false,
  "error": {
    "code": "DEPENDENCY_CYCLE",
    "message": "Dependency cycle detected",
    "details": {
      "child": "tsq-c11111",
      "blocker": "tsq-a11111"
    }
  }
}
```

## Core Semantics

- ID resolution: exact match first; else unique prefix; ambiguous prefix errors; `--exact-id` enforces full match.
- Status alias: `done` accepted by CLI, normalized to `closed`.
- Claim: strict CAS; only unassigned tasks can be claimed.
- Claim `--require-spec`: blocks claim unless `tsq spec check <id>` would return `ok: true`.
- External refs: `external_ref` is optional task metadata; list filtering is exact-match (`--external-ref`) and search supports `external_ref:<value>`.
- Discovered-from: `discovered_from` is optional provenance metadata; it is non-blocking and supports create/update/list/search (`discovered_from:<id>`).
- Watch: defaults to `open,in_progress` with `--interval 2` seconds; `--once` emits a single frame for scripting.
- Ready: task is unblocked and not in `closed|canceled|deferred`; `--lane planning` returns `planning_state=needs_planning`, `--lane coding` returns `planning_state=planned`.
- Stale: returns tasks where `updated_at <= now - days` (default statuses: `open|in_progress|blocked|deferred`).
- Dependency types: `blocks` and `starts_after`.
- Ready semantics: only `blocks` edges gate readiness (blocker must be `closed|canceled`); `starts_after` is non-blocking ordering metadata.
- Dependency add: self-edge rejected for all types; cycle detection applies to `blocks` graph only.
- Search dep fields: use `dep_type_in:<blocks|starts_after>` and `dep_type_out:<blocks|starts_after>`; `dep_type:<...>` is rejected as ambiguous.
- Relation add/remove: self-edge rejected; `relates_to` maintained bidirectionally.
- Duplicate workflow: `duplicate` sets `duplicate_of`, adds `duplicates` link metadata, closes source, and does not rewire dependencies.
- Duplicates scaffold: `duplicates` reports normalized-title candidate groups without mutating state.
- Merge workflow: `merge` consolidates duplicates into a target; `--dry-run` previews projected outcomes and plan summary without writes.
- Supersede: source task closed + `superseded_by` set; replacement task unchanged; dependencies not rewired.
- Spec check: returns diagnostics and checks canonical spec fingerprint drift plus required sections (`Overview`, `Constraints / Non-goals`, `Interfaces (CLI/API)`, `Data model / schema changes`, `Acceptance criteria`, `Test plan`).

## Locking + Durability Notes

- Single-writer lock via `.tasque/.lock` (`open wx`).
- Lock timeout: `3s`; retry jitter: `20-80ms`.
- Stale lock cleanup only when lock host matches current host and lock PID is dead.
- Event appends are fsynced; event log is append-only.
- `state.json`, snapshots, specs, and config writes are atomic temp-write + rename.
- Snapshot writes keep only the latest 5 snapshot JSON files (oldest pruned best-effort).
- Snapshot load falls back newest-to-oldest and ignores invalid snapshot files with a warning.
- Replay recovery tolerates one malformed trailing JSONL line in `events.jsonl` (ignored with warning).
- `events.jsonl` remains canonical source of truth.
