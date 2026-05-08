# Task Authoring Checklist

Read when: creating/refining tasks or splitting work for parallel execution.

## Minimum quality bar

- Use clear, action-oriented titles.
- Set `kind` correctly (`task`, `feature`, `epic`).
- Assign priority (`0..3`) intentionally.
- Add labels using a consistent format.
- Attach spec content when scope/acceptance is non-trivial.
- Link task dependencies and relations explicitly.

## Parallelization guidance

- Prefer multiple independent tasks over one large task.
- Use `blocks` only when completion truly gates another task.
- Use `starts_after` for ordering without readiness blocking.
- Add discovered follow-up work as new tasks with `--discovered-from <id>`.
- Keep each task sized so one agent can complete it in one focused pass.

## Practical starter flow

```bash
tsq create "<title>" --kind task -p 2 --needs-plan
tsq spec <id> --text "<markdown spec>"
tsq block <child> by <blocker>
tsq relate <src> <dst>
```

For many tasks, write `tasks.md` with nested two-space bullets:

```md
- Parent task
  - Child task
    - Grandchild task
- [ ] Another parent task
```

```bash
tsq create --parent <id> --from-file tasks.md
```
