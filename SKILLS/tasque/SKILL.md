---
name: tasque
description: Operational guide for Tasque (tsq) local task tracking and management.
---

<!-- tsq-managed-skill:v1 -->

Tasque = durable, local-first task memory for agent work.
Default day-to-day playbook.

## When to use tsq

- Use `tsq` for multi-step, multi-session, dependency-blocked, shared-agent work.
- Use transient checklist for short linear single-session work.

## Session routine (default)

```bash
tsq find ready --lane planning
tsq find ready --lane coding
tsq find blocked
```

Pick one task; inspect context:

```bash
tsq show <id>
```


### 1) Capture new work

```bash
tsq create "Implement <feature>" --kind feature -p 1 --needs-plan
tsq create "Fix <bug>" --kind task -p 1 --needs-plan
```

Planning already done:

```bash
tsq create "Implement <feature>" --planned
```

### 2) Split parent into many children

```bash
tsq create --parent <parent-id> \
  "Design API contract" \
  "Implement service logic" \
  "Add regression tests"
```

Shared defaults for all children:

```bash
tsq create --parent <parent-id> --kind task -p 2 --planned \
  "Wire CLI args" \
  "Update docs" \
  "Add integration tests"
```

Safe reruns without duplicate children:

```bash
tsq create --parent <parent-id> --ensure \
  "Wire CLI args" \
  "Update docs" \
  "Add integration tests"
```

Batch from `tasks.md`:

```md
- Add parser tests
  - Cover nested task hierarchy
  - Cover invalid indentation
- Wire CLI command
- Update skill docs
```

```bash
tsq create --parent <parent-id> --from-file tasks.md
```

Use `tsq create --parent <id> --from-file tasks.md` for many tasks.

### 3) Planning handoff -> coding

```bash
tsq spec <id> --text "## Plan\n...\n## Acceptance\n..."
tsq planned <id>
tsq claim <id> --assignee <name> --start
```

Use `tsq spec <id> --show` when you need spec markdown from sync worktree.

### 4) Model deps for parallel agents

Hard blocker (changes readiness):

```bash
tsq block <child-id> by <blocker-id>
```

Soft ordering only:

```bash
tsq order <later-id> after <earlier-id>
```

Check actionable tasks:

```bash
tsq find ready --lane coding
tsq find ready --lane planning
```

### 5) Capture discovered follow-up work

```bash
tsq create "Handle edge case <x>" --discovered-from <current-id> --needs-plan
tsq relate <new-id> <current-id>
```

### 5b) Idempotent root/parent create for automation

```bash
tsq create "Implement auth module" --ensure
tsq create --parent <parent-id> "Add tests" --ensure
```

`--ensure` returns existing task when same normalized title already exists under the same parent.

### 6) Park / unpark work

```bash
tsq defer <id> --note "waiting"
tsq find deferred
tsq open <id>
```

### 7) Resolve duplicate/superseded work

```bash
tsq duplicate <id> of <canonical-id> --note "same root issue"
tsq duplicates
tsq merge <source-id...> --into <target-id> --dry-run
tsq merge <source-id...> --into <target-id> --force
tsq supersede <old-id> with <new-id> --note "replaced approach"
```

### 8) Close / report

```bash
tsq done <id> --note "verified"
tsq history <id> --limit 20
tsq find open --tree
```

Use `--format json` only when scripting/parsing; human output is fine for inspection.

## Built-in task authoring checklist

### Minimum quality bar

- Titles: clear, action-oriented (verb + object + scope).
- Set `kind`: `task|feature|epic`.
- Set priority intentionally: `0..3`.
- Add labels with consistent naming.
- Attach spec when scope/acceptance non-trivial.
- Add explicit deps/relations when relevant.

### Parallelization guidance

- Prefer multiple independent tasks over one large task.
- Use `blocks` only when work truly gates another task.
- Use `starts_after` for sequencing without blocking readiness.
- Add discovered work as new tasks via `--discovered-from`.
- Keep each task small enough for one focused agent pass.

### Practical authoring starter

```bash
tsq create "<title>" --kind task -p 2 --needs-plan
tsq spec <id> --text "<markdown spec>"
tsq block <child> by <blocker>
tsq relate <src> <dst>
```

## Required habits

- Keep lifecycle `status` and `planning_state` separate.
- Use deps to make parallel execution explicit.
- Create follow-up tasks; avoid chat TODOs.
- Prefer `--format json` for automation; inspect with human output.
- Use `--ensure` in scripts to prevent duplicate creates on rerun.

## Read when needed

- Planning/deferred semantics: `references/planning-workflow.md`
- JSON schema + durability details: `references/machine-output-and-durability.md`
- Full option matrix (edge cases): `references/command-reference.md`
- Install if missing: `npm install -g @bumpyclock/tasque`
