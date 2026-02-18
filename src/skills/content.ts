import type { SkillTarget } from "./types";

export const MANAGED_MARKER = "tsq-managed-skill:v1";

export function renderSkillMarkdown(skillName: string): string {
  return `---
name: ${skillName}
description: Operational guide for Tasque (tsq) local task tracking
---

<!-- ${MANAGED_MARKER} -->

# Tasque Skill

Use \`tsq\` for durable local task tracking.

## Core loop

1. \`tsq ready\`
2. \`tsq show <id>\`
3. \`tsq update <id> --status in_progress\`
4. \`tsq update <id> --status closed\`

## Create and inspect

- \`tsq create "Title" --kind task|feature|epic -p 0..3\`
- \`tsq show <id>\`
- \`tsq list [--status S] [--assignee A] [--kind K] [--label L] [--tree [--full]]\`
- \`tsq search "status:open label:bug some title text"\`
- \`tsq doctor\`

## Close and reopen

- \`tsq close <id> [<id2> ...] [--reason <text>]\` — close one or more tasks
- \`tsq reopen <id> [<id2> ...]\` — reopen closed tasks (not canceled)

## History

- \`tsq history <id> [--limit N] [--type <event-type>] [--actor <name>] [--since <iso>]\`

## Specs

- \`tsq spec attach <id> --text "<markdown>"\` for short inline specs
- \`tsq spec attach <id> --file <path>\` to ingest an existing markdown file
- \`tsq spec attach <id> --stdin\` to ingest piped markdown content
- \`tsq spec check <id>\` to validate fingerprint + required sections diagnostics
- \`tsq update <id> --claim --require-spec\` to enforce a passing spec check before claiming
- Never manually write \`.tasque/specs/<id>/spec.md\`; always use \`tsq spec attach\` so canonical path + metadata stay consistent

## Labels

- \`tsq label add <id> <label>\` — add label (lowercase, [a-z0-9:_/-], max 64)
- \`tsq label remove <id> <label>\`
- \`tsq label list\` — all labels with counts

## Dependencies and relations

- \`tsq dep add <child> <blocker>\` means child waits on blocker
- \`tsq dep remove <child> <blocker>\`
- \`tsq dep tree <id> [--direction up|down|both] [--depth N]\` — dependency graph
- \`tsq link add <src> <dst> --type relates_to|replies_to|duplicates|supersedes\`
- \`tsq link remove <src> <dst> --type relates_to|replies_to|duplicates|supersedes\`
- \`tsq supersede <old-id> --with <new-id> [--reason <text>]\`

## Search

- \`tsq search "<query>"\` — structured query with implicit AND
- Fields: \`id\`, \`title\`, \`status\`, \`kind\`, \`priority\`, \`assignee\`, \`parent\`, \`label\`, \`ready\`
- Negation: \`-status:closed\` (use \`--\` separator: \`tsq search -- -status:closed\`)
- Bare words match title substring

## JSON mode

Add \`--json\` to any command for stable automation output:
\`{"schema_version":1,"command":"tsq ...","ok":true,"data":{}}\`

## Restart durability

- Canonical history is append-only: \`.tasque/events.jsonl\`
- Derived cache is rebuildable: \`.tasque/tasks.jsonl\`
- State recovers by replaying snapshot + event tail after restart
`;
}

export function renderReadmeMarkdown(skillName: string, target: SkillTarget): string {
  return `<!-- ${MANAGED_MARKER} -->
# ${skillName} Skill

Managed skill package for \`${target}\`.

## Files

- \`SKILL.md\`: operational guide for \`tsq\`
- \`README.md\`: installer metadata and summary

## Safety

This directory is managed by Tasque skill installer. Remove marker \`${MANAGED_MARKER}\` to opt out of managed updates.
`;
}
