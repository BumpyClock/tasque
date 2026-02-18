# Decision Record: Typed Dependency Model

## Status

Deferred

## Context

The current dependency model uses `Record<taskId, string[]>` — a simple mapping from child task to its blocker task IDs. All dependencies have the same semantics: "child is blocked until blocker is closed/canceled."

A typed dependency model would allow different blocking semantics:

- `blocks` / `blocked_by` (current default)
- `needs_review_from` — softer blocking with different resolution rules
- `starts_after` — sequencing without strict blocking
- `discovered_from` — provenance tracking (see separate decision record)

## Decision

Defer typed dependencies until `planning_state` stabilization and real-world usage reveals which dependency types are actually needed.

## Rationale

- The current untyped model is simple and covers the primary use case
- Adding types to the dependency model requires:
  - Extending `State.deps` from `Record<string, string[]>` to `Record<string, Array<{blocker: string, type: string}>>`
  - Updating `isReady` / `listReady` to handle type-specific resolution rules
  - Backward-compatible event migration for existing `dep.added` events
  - CLI UX for specifying dependency type
- Premature abstraction risk: without real usage data, we might design for hypothetical needs

## Future Options

1. **Typed edges**: `dep add <child> <blocker> --type needs_review_from`
2. **Edge metadata**: attach metadata to existing untyped edges
3. **Hybrid**: keep simple blocking deps, use relation links for non-blocking relationships

## Recommended Path

Option 1 with backward-compatible defaults — existing `dep.added` events without a type field default to `blocks`.

## Sequencing

- Depends on `planning_state` stabilization
- Should be designed together with `discovered_from` dependency tracking
- Consider as part of a broader "dependency model v2" effort

## Revisit Trigger

When users report needing different blocking semantics, or when the `planning_state` + lane-aware ready workflow is stable and there's demand for richer dependency classification.
