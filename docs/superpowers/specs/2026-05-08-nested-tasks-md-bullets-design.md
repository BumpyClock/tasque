# Nested `tasks.md` Bullets Design

## Summary

`tsq create --from-file tasks.md` will accept nested Markdown bullets and create a real Tasque parent/child hierarchy. This extends the verb-first batch create workflow so agents can write task breakdowns in the same shape they naturally think and communicate them.

## Goals

- Support nested `tasks.md` bullets as task hierarchy.
- Preserve current flat bullet behavior.
- Keep output schema stable: `data.tasks[]` remains a flat creation-ordered array.
- Provide line-numbered validation errors for malformed files.
- Update CLI help, docs, and Tasque skill guidance.

## Non-Goals

- Support arbitrary Markdown documents.
- Support non-bullet headings, paragraphs, or code blocks.
- Add tree-shaped JSON output for create.
- Add labels, dependency syntax, status syntax, or assignee syntax inside `tasks.md`.
- Support tab indentation.

## Input Format

Accepted bullet markers:

```md
- Task title
- [ ] Checkbox title
- [x] Completed checkbox title
- [X] Completed checkbox title
```

Nested bullets use exactly two spaces per depth:

```md
- Parent A
  - Child A1
    - Grandchild A1a
  - [ ] Child A2
- Parent B
```

Blank lines are ignored.

## Hierarchy Mapping

Each bullet creates one task. Parent assignment is derived from indentation:

- Depth `0` bullet creates a root task.
- Depth `1` bullet becomes child of nearest previous depth `0` bullet.
- Depth `2` bullet becomes child of nearest previous depth `1` bullet.
- Deeper levels follow same stack rule.
- Creation order is depth-first and follows file order.

With `--parent <id>`:

- Depth `0` bullets become children of `<id>`.
- Depth `1` bullets become grandchildren of `<id>`.
- Deeper levels continue normally.

Example:

```bash
tsq create --parent tsq-epic1234 --from-file tasks.md
```

```md
- Parent A
  - Child A1
- Parent B
```

Creates:

- `Parent A`, parent `tsq-epic1234`
- `Child A1`, parent `Parent A`
- `Parent B`, parent `tsq-epic1234`

## Output

JSON output remains:

```json
{
  "data": {
    "tasks": [
      { "id": "tsq-abc", "title": "Parent A", "parent_id": null },
      { "id": "tsq-def", "title": "Child A1", "parent_id": "tsq-abc" }
    ]
  }
}
```

If `--parent` is used, top-level created tasks have that parent id.

Human output remains one rendered task per created task. No new tree rendering is required for create output.

## Validation

Reject with `VALIDATION_ERROR` and line number when possible:

- tab indentation
- odd number of leading spaces
- indentation jump by more than one level
- indented first bullet
- non-bullet content
- empty bullet title after checkbox stripping

Examples:

```md
- Parent
    - Grandchild without child
```

Reject: line 2 jumps from depth 0 to depth 2.

```md
- Parent
 - Odd indent
```

Reject: line 2 has odd indentation.

Existing argument conflicts remain:

- `--from-file` conflicts with positional titles.
- `--from-file` conflicts with `--id`.
- `--planned` conflicts with `--needs-plan`.
- `--description`, `--body-file`, and `--id` require exactly one created task.
- `--ensure` conflicts with `--id`.

## Ensure Semantics

`--ensure` applies to each bullet title under its resolved parent. Two siblings with the same title resolve to the same existing sibling task if present. Same title under different parents can create or resolve different tasks.

## Implementation Shape

Replace `parse_task_bullets(path) -> Vec<String>` with a parser that returns creation items:

```rust
struct ParsedTaskBullet {
    title: String,
    depth: usize,
    line_no: usize,
}
```

The create command then maintains a stack of created or ensured parent ids:

- stack index `0` is the current depth `0` created task id.
- after creating depth `N`, truncate stack to `N`, then push the created id.
- parent for depth `0` is `--parent`.
- parent for depth `N > 0` is `stack[N - 1]`.

This keeps hierarchy logic in CLI create orchestration and avoids changing domain storage.

## Tests

Add or update integration tests for:

- nested file creates parent/child/grandchild ids and `parent_id`s.
- `--parent` makes top-level bullets children of external parent.
- checkbox bullets work at nested levels.
- `--ensure` rerun does not duplicate nested tasks.
- tabs rejected with line number.
- odd indent rejected with line number.
- skipped depth rejected with line number.
- non-bullet content rejected with line number.
- output shape remains flat `data.tasks[]` in creation order.

## Docs

Update:

- `tsq create --help` examples.
- README command reference.
- npm README command reference.
- `SKILLS/tasque/SKILL.md`.
- `SKILLS/tasque/references/command-reference.md`.
- `SKILLS/tasque/references/task-authoring-checklist.md`.
- existing verb-first design spec batch-create section.

Docs should show this canonical `tasks.md` format:

```md
- Parent task
  - Child task
    - Grandchild task
- [ ] Another parent task
```

## Acceptance Criteria

- `tsq create --from-file tasks.md` accepts nested two-space bullets.
- `tsq create --parent <id> --from-file tasks.md` preserves external parent root.
- JSON and human output remain stable.
- Invalid indentation/content returns clear validation error with line number.
- Release gates pass: fmt, clippy, tests, release build.
