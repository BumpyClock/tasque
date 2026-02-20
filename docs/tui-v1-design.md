# TUI v1 Design (Read-Only)

## Overview
- Trigger: running `tsq` with no args in an interactive TTY opens TUI mode.
- Scope: read-only presentation only (no task mutation commands inside TUI).
- Primary views:
  - `List`: richer `watch`-style live feed with selection.
  - `Board`: kanban-style grouped by status.

## Entry and Fallback Rules
- `tsq` (no args, stdin+stdout TTY): open TUI.
- `tsq` (no args, non-TTY): keep CLI help behavior.
- `tsq --json` (no command): return validation error; do not open TUI.
- Explicit command available: `tsq tui`.

## Interaction Model (v1)
- `q` / `Ctrl+C`: quit.
- `Tab`: switch List <-> Board.
- `r`: refresh now.
- `p`: pause/resume refresh loop.
- `Up` / `Down`: move selected row/card.

## Layout
- Header: mode, refreshed timestamp, interval, active filters.
- Summary row: active/open/in_progress/blocked counts and selected task id.
- Main body:
  - List view: ordered rows with status, id, title, meta badge.
  - Board view: status buckets with task rows under each bucket.
- Inspector: selected task metadata (`id`, `title`, status/kind/priority/planning, assignee/parent/labels, timestamps).
- Footer: key hints.

## Data and Refresh
- Refresh source: `service.list(...)` with status/assignee filters.
- Default interval: 2s, valid range: 1..60.
- Refresh failures are non-fatal in loop mode: keep last good frame visible and print error.

## Non-Goals (v1)
- No create/update/claim/close/merge actions.
- No inline editing.
- No advanced filtering UI beyond CLI flags (`--status`, `--assignee`, `--board`, `--once`).
