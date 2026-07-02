# Tasque OpenTUI (Read-Only)

OpenTUI React app for Tasque with four tabs:
- `Tasks`: task tree with planning/spec/updated metadata
- `Epics`: one progress row per epic (done/in-progress/open counts)
- `Board`: open / in-progress / done lanes
- `Deps`: dependency tree of the selected task

The app is read-only. It does not mutate Tasque data.

## Data source

There is a single data source: the TUI spawns the `tsq` CLI with `--json` and
parses the standard envelope. There is no fallback to `.tasque/state.json` or
`.tasque/events.jsonl`.

Commands used:
- `tsq --json watch --once --status <csv> [--assignee <name>]` — task list (every refresh)
- `tsq --json deps <id> --direction both --depth 4` — dependency tree (Deps tab)
- `tsq --json spec <id> --show` — spec content (spec dialog)

## Environment variables

- `TSQ_TUI_BIN`: path to the `tsq` binary (default: `tsq` on `PATH`)
- `TSQ_TUI_INTERVAL`: refresh interval in seconds, clamped to 1-60 (default: `2`)
- `TSQ_TUI_STATUS`: status CSV passed to `watch --once` and used for the first
  filter preset (default: `open,in_progress,blocked,deferred,closed,canceled`;
  tabs filter client-side, so narrowing this hides tasks from Board/Epics too)
- `TSQ_TUI_ASSIGNEE`: filter tasks by assignee
- `TSQ_TUI_VIEW`: initial tab; one of `tasks`, `epics`, `board`, `deps` (default: `tasks`)

## Spec state

Every row/card includes spec state derived from task metadata:
- `attached`: `spec_path` and `spec_fingerprint` both present
- `missing`: neither present
- `invalid`: only one present

## Run

```bash
cd tui-opentui
bun install
bun run start
```

## Test

```bash
bun test                                        # unit tests; contract test skips
TSQ_CONTRACT_BIN=/abs/path/to/tsq bun test      # also runs real-binary contract test
```

## Keyboard

- `q` or `Esc`: quit
- `Ctrl+C`: quit
- `Tab`: next tab
- `1` / `2` / `3` / `4`: jump to Tasks / Epics / Board / Deps
- `Up` / `Down` or `j` / `k`: move selection
- `h` / `l` or `Left` / `Right`: switch board lane (Board tab only)
- `f`: cycle filter preset
- `Enter`: open spec dialog for the selected task (when a spec is attached)
- `r`: refresh now

Spec dialog:
- `Esc` / `Enter` / `q`: close
- `j` / `k` or `Up` / `Down`: scroll
- `PgUp` / `PgDn`: page
- `Home` / `End`: jump to start/end
