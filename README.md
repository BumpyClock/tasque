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

## Build And Release

```bash
bun run build
bun run release
```

- `build` compiles single-file binary to `dist/tsq` (or `dist/tsq.exe` on Windows).
- `release` rebuilds, writes platform artifact, generates task-derived release notes, then writes checksums under `dist/releases/`.
- release artifacts:
  - `tsq-v<version>-<platform>-<arch>[.exe]`
  - `RELEASE_NOTES.md`
  - `RELEASE_NOTES.json`
  - `SHA256SUMS.txt` (contains checksums for all artifacts above)
- release notes baseline uses latest git tag timestamp; with no tag, all closed tasks are considered.
- set `TSQ_BIN` to override which installed `tsq` executable release hooks call.

## Storage Layout

Repo-local `.tasque/`:

- `events.jsonl`: canonical append-only event log.
- `tasks.jsonl`: derived projection cache (rebuildable, gitignored).
- `snapshots/`: periodic checkpoints (gitignored by default).
- `specs/<task-id>/spec.md`: canonical markdown specs attached to tasks.
- `config.json`: config (`snapshot_every` default `200`).
- `.lock`: ephemeral write lock.
- `.gitignore`: local-only artifacts (`tasks.jsonl`, `.lock`, `snapshots/`, temp files).

Recommended commit policy:

- Commit `.tasque/events.jsonl` and `.tasque/config.json`.
- Do not commit `.tasque/tasks.jsonl`.

## Command List

Global options:

- `--json`: JSON envelope output.
- `--exact-id`: disable partial ID resolution.

Commands:

- `tsq init`
- `tsq create "<title>" [--kind task|feature|epic] [-p|--priority 0..3] [--parent <id>] [--description <text>] [--external-ref <ref>]`
- `tsq show <id>`
- `tsq list [--status <open|in_progress|blocked|closed|canceled|done>] [--assignee <name>] [--external-ref <ref>] [--kind <task|feature|epic>] [--label <label>] [--label-any <csv-or-repeat>] [--created-after <iso>] [--updated-after <iso>] [--closed-after <iso>] [--unassigned] [--id <csv-or-repeat>] [--tree] [--full]`
- `tsq ready`
- `tsq watch [--once] [--interval <seconds>] [--status <csv>] [--assignee <name>] [--tree]`
- `tsq stale [--days <n>] [--status <open|in_progress|blocked|closed|canceled|done>] [--assignee <name>]`
- `tsq doctor`
- `tsq update <id> [--title <text>] [--status <...>] [--priority <0..3>] [--description <text>] [--clear-description] [--external-ref <ref>] [--clear-external-ref]`
- `tsq update <id> --claim [--assignee <name>] [--require-spec]`
- `tsq note add <id> <text>`
- `tsq note list <id>`
- `tsq spec attach <id> [source] [--file <path> | --stdin | --text <markdown>]`
- `tsq spec check <id>`
- `tsq dep add <child> <blocker>`
- `tsq dep remove <child> <blocker>`
- `tsq link add <src> <dst> --type <relates_to|replies_to|duplicates|supersedes>`
- `tsq link remove <src> <dst> --type <relates_to|replies_to|duplicates|supersedes>`
- `tsq duplicate <id> --of <canonical-id> [--reason <text>]`
- `tsq duplicates [--limit <n>]` (dry-run scaffold)
- `tsq supersede <old-id> --with <new-id> [--reason <text>]`

Tree mode notes:

- `--tree` defaults to showing only `open` and `in_progress` tasks.
- Use `--tree --full` to include all statuses in tree output.
- `>=120` columns: metadata stays inline with title.
- `90-119` columns: metadata moves to a second line when needed.
- `<90` columns: metadata always uses a second line, and long titles are truncated.

Skill installer via `init`:

- `tsq init --install-skill`
- `tsq init --uninstall-skill`
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
- Watch: defaults to `open,in_progress` with `--interval 2` seconds; `--once` emits a single frame for scripting.
- Ready: task status in `open|in_progress` and no open blockers.
- Stale: returns tasks where `updated_at <= now - days` (default statuses: `open|in_progress|blocked`).
- Open blocker: blocker exists and status is not `closed|canceled`.
- Dependency add: self-edge/cycle rejected.
- Relation add/remove: self-edge rejected; `relates_to` maintained bidirectionally.
- Duplicate workflow: `duplicate` sets `duplicate_of`, adds `duplicates` link metadata, closes source, and does not rewire dependencies.
- Duplicates scaffold: `duplicates` reports normalized-title candidate groups without mutating state.
- Supersede: source task closed + `superseded_by` set; replacement task unchanged; dependencies not rewired.
- Spec check: returns diagnostics and checks canonical spec fingerprint drift plus required sections (`Overview`, `Constraints / Non-goals`, `Interfaces (CLI/API)`, `Data model / schema changes`, `Acceptance criteria`, `Test plan`).

## Locking + Durability Notes

- Single-writer lock via `.tasque/.lock` (`open wx`).
- Lock timeout: `3s`; retry jitter: `20-80ms`.
- Stale lock cleanup only when lock host matches current host and lock PID is dead.
- Event appends are fsynced; event log is append-only.
- `tasks.jsonl`, snapshots, specs, and config writes are atomic temp-write + rename.
- Snapshot writes keep only the latest 5 snapshot JSON files (oldest pruned best-effort).
- Snapshot load falls back newest-to-oldest and ignores invalid snapshot files with a warning.
- Replay recovery tolerates one malformed trailing JSONL line in `events.jsonl` (ignored with warning).
- `events.jsonl` remains canonical source of truth.
