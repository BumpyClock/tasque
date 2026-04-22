# Command Reference

Read when: you need exact command syntax or available options.

## Core workflow

- `tsq` (no args, TTY): open read-only TUI
- `tsq init [--wizard|--no-wizard] [--yes] [--preset <name>] [--sync-branch <branch>]`
- `tsq init --install-skill|--uninstall-skill [--skill-targets ...] [--skill-name <name>] [--force-skill-overwrite]`

In git repos, `tsq init` defaults to sync-worktree mode using `tasque-sync`.
Use `--sync-branch <branch>` to choose another branch. Existing main-tree
`.tasque` data migrates automatically. Non-git directories use local `.tasque/`
storage.

- `tsq create [<title>] [--child <title> ...] [--kind ...] [-p ...] [--parent <id>] [--description <text>] [--external-ref <ref>] [--discovered-from <id>] [--planning <needs_planning|planned>] [--needs-planning] [--ensure] [--id <tsq-xxxxxxxx>] [--body-file <path|->]`
- `tsq show <id>`
- `tsq list [--status ...] [--assignee ...] [--unassigned] [--external-ref <ref>] [--discovered-from <id>] [--kind ...] [--label ...] [--label-any ...] [--created-after <iso>] [--updated-after <iso>] [--closed-after <iso>] [--id <id,...>] [--planning <needs_planning|planned>] [--dep-type <blocks|starts_after>] [--dep-direction <in|out|any>] [--tree] [--full]`
- `tsq search <query>`
- `tsq update <id> [--title ...] [--description ...] [--clear-description] [--status ...] [--priority ...] [--external-ref <ref>] [--clear-external-ref] [--discovered-from <id>] [--clear-discovered-from] [--planning <needs_planning|planned>]`
- `tsq update <id> --claim [--assignee <a>] [--require-spec]`
- `tsq close <id...> [--reason <text>]`
- `tsq reopen <id...>`
- `tsq ready [--lane <planning|coding>]`

## Dependencies and relations

- `tsq dep add <child> <blocker> [--type <blocks|starts_after>]`
- `tsq dep remove <child> <blocker> [--type <blocks|starts_after>]`
- `tsq dep tree <id> [--direction <up|down|both>] [--depth <n>]`
- `tsq link add <src> <dst> --type <relates_to|replies_to|duplicates|supersedes>`
- `tsq link remove <src> <dst> --type <relates_to|replies_to|duplicates|supersedes>`
- `tsq duplicate <id> --of <canonical-id> [--reason <text>]`
- `tsq duplicates [--limit <n>]`
- `tsq merge <source-id...> --into <target-id> [--reason <text>] [--force] [--dry-run]`
- `tsq supersede <old-id> --with <new-id> [--reason <text>]`

## Specs, notes, labels, history

- `tsq spec attach <id> [source] [--file <path> | --stdin | --text <markdown>]`
- `tsq spec check <id>`
- `tsq note add <id> <text>`
- `tsq note list <id>`
- `tsq label add <id> <label>`
- `tsq label remove <id> <label>`
- `tsq label list`
- `tsq history <id> [--limit <n>] [--type <event-type>] [--actor <name>] [--since <iso>]`

## Reporting and maintenance

- `tsq watch [--once] [--interval <seconds>] [--status <csv>] [--assignee <name>] [--tree]`
- `tsq tui [--once] [--interval <seconds>] [--status <csv>] [--assignee <name>] [--board|--epics]`
- `tsq stale [--days <n>] [--status <status>] [--assignee <name>] [--limit <n>]`
- `tsq orphans`
- `tsq doctor`
- `tsq repair [--fix] [--force-unlock]`
- `tsq sync [--no-push]`
- `tsq hooks install [--force]`
- `tsq hooks uninstall`
- `tsq migrate --sync-branch <branch>`
- `tsq merge-driver <ancestor> <ours> <theirs>`

## Global options and status alias

- Add `--json` to any command for stable machine output.
- Add `--exact-id` to disable fuzzy id matching.
- Status alias: `done` maps to `closed`.
