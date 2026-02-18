# Decision Record: discovered-from Dependency Tracking

## Status

Deferred

## Context

When a task is created as a result of work on another task (e.g., a bug found while implementing a feature), it would be useful to track the "discovered from" relationship. This is a specialized dependency type that differs from `blocks`/`blocked_by` semantics.

## Decision

Defer this feature until after `planning_state` rollout stabilizes.

## Rationale

- The current relation model (`relates_to`, `duplicates`, `supersedes`, `replies_to`) covers most cross-task relationships
- Adding `discovered_from` requires deciding whether it should be a relation link or a dependency edge
- The `planning_state` axis provides the more immediately needed workflow structure
- Adding too many relation types risks confusion without clear workflow benefits

## Future Options

1. **Relation link**: `link add <new-task> <source-task> --type discovered_from` — lightweight, no blocking semantics
2. **Metadata field**: `discovered_from: <task-id>` on the Task record — similar to `replies_to`
3. **Automated**: infer from creation context (e.g., if created while another task is `in_progress`)

## Recommended Path

Option 2 (metadata field) aligns with the existing `replies_to` pattern and avoids expanding the relation type enum.

## Revisit Trigger

After `planning_state` has been used in production for 2+ weeks and the workflow is stable.
