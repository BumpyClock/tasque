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
- \`tsq list --status open --kind task\`
- \`tsq doctor\`

## Dependencies and relations

- \`tsq dep add <child> <blocker>\` means child waits on blocker
- \`tsq dep remove <child> <blocker>\`
- \`tsq link add <src> <dst> --type relates_to|replies_to|duplicates|supersedes\`
- \`tsq link remove <src> <dst> --type relates_to|replies_to|duplicates|supersedes\`
- \`tsq supersede <old-id> --with <new-id> [--reason <text>]\`

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
