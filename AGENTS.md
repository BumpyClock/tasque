# Tasque Agent Guide (V1 Shipped)

- assume tsq is already built and installed in path. if not build and install it. 
- use parallel sub-agents and agent teams for implementation.

## Objective
Simple local-first tracker for local agent work, inspired by Beads patterns.
Local-first. Git-friendly. JSONL-backed.
Durable across restarts and context compaction.

## Scope (Current)
- task/feature/epic create/read/update
- typed dependencies (`blocks`, `starts_after`)
- relation links
- duplicate workflow (`duplicate`, `duplicates` dry-run scaffold)
- merge workflow (`merge` with `--force` and `--dry-run`)
- ready detection
- lane-aware ready detection (`--lane planning|coding`)
- planning state tracking (`planning_state`)
- deferred lifecycle status for parked work
- atomic claim
- optional claim spec gate (`--require-spec`)
- spec attach/check workflow
- supersede workflow
- orphans reporting (`tsq orphans`, read-only)
- append-only audit trail
- stable machine output (`--json`)
- tree list view (`tsq list --tree`)
- skill install/uninstall via `tsq init`

## Non-Goals
- sqlite/dolt backends
- remote sync service
- background daemon
- multi-machine consistency

## Stack
- Runtime: Rust
- Language: Rust
- TUI : OpenTUI, use `opentui` skill. 
- Validation: strongly typed domain + parser checks
- Output: JSON envelopes + terminal rendering

## Detailed Reference
For storage model, task model, CLI contract, JSON output format, durability rules, repo conventions, and finishing-task checklists, see [AGENTS-reference.md](./AGENTS-reference.md).