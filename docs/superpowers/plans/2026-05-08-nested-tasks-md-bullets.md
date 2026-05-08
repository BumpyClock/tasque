# Nested `tasks.md` Bullets Implementation Plan

> **For agentic workers:** Prefer `subagent-driven-development` for execution when available. Task implementers own task work and review fixes; integration owner owns final integration. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add nested Markdown bullet support to `tsq create --from-file tasks.md`, mapping indentation to real Tasque parent/child hierarchy.

**Architecture:** Keep parsing and create orchestration inside `src/cli/commands/task_create.rs` because this is CLI input syntax, not domain storage. The parser returns flat `ParsedTaskBullet` items with title, depth, and line number; create execution walks those items in file order, creating tasks and maintaining a depth-indexed parent stack. Existing service APIs remain unchanged.

**Tech Stack:** Rust 2024, clap CLI, Tasque service layer, JSON envelope integration tests.

---

## File Map

- Modify `src/cli/commands/task_create.rs`: replace flat bullet parsing with nested bullet parser and hierarchy-aware create loop.
- Modify `tests/verb_first_task_commands.rs`: add nested happy-path and validation coverage.
- Modify `tests/create_children_batch.rs`: extend ensure coverage to nested `--from-file` batches.
- Modify `README.md`, `npm/README.md`, `SKILLS/tasque/SKILL.md`, `SKILLS/tasque/references/command-reference.md`, `SKILLS/tasque/references/task-authoring-checklist.md`: document nested `tasks.md`.
- Modify `docs/superpowers/specs/2026-05-08-verb-first-cli-ergonomics-design.md`: update older v1 batch-create section so it no longer says nested bullets are rejected.

---

### Task 1: Add Nested Parser Tests

**Files:**
- Modify: `tests/verb_first_task_commands.rs`

- [ ] **Step 1: Add nested `--from-file` creation test**

Append this test near `create_from_file_accepts_markdown_bullets`:

```rust
#[test]
fn create_from_file_maps_nested_bullets_to_parent_ids() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(
        &file,
        "- Parent A\n  - Child A1\n    - Grandchild A1a\n  - [ ] Child A2\n- Parent B\n",
    )
    .unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"].as_array().expect("tasks");
    assert_eq!(tasks.len(), 5);
    assert_eq!(tasks[0]["title"].as_str(), Some("Parent A"));
    assert_eq!(tasks[1]["title"].as_str(), Some("Child A1"));
    assert_eq!(tasks[2]["title"].as_str(), Some("Grandchild A1a"));
    assert_eq!(tasks[3]["title"].as_str(), Some("Child A2"));
    assert_eq!(tasks[4]["title"].as_str(), Some("Parent B"));

    let parent_a = tasks[0]["id"].as_str().expect("parent id");
    let child_a1 = tasks[1]["id"].as_str().expect("child id");
    assert!(tasks[0].get("parent_id").is_none());
    assert_eq!(tasks[1]["parent_id"].as_str(), Some(parent_a));
    assert_eq!(tasks[2]["parent_id"].as_str(), Some(child_a1));
    assert_eq!(tasks[3]["parent_id"].as_str(), Some(parent_a));
    assert!(tasks[4].get("parent_id").is_none());
}
```

- [ ] **Step 2: Add external parent test**

Append:

```rust
#[test]
fn create_from_file_nested_bullets_respect_external_parent() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let external_parent = create_task(repo.path(), "Epic");
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- Parent A\n  - Child A1\n- Parent B\n").unwrap();

    let result = run_json(
        repo.path(),
        ["create", "--parent", &external_parent, "--from-file", "tasks.md"],
    );

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"].as_array().expect("tasks");
    assert_eq!(tasks.len(), 3);
    let parent_a = tasks[0]["id"].as_str().expect("parent A id");
    assert_eq!(tasks[0]["parent_id"].as_str(), Some(external_parent.as_str()));
    assert_eq!(tasks[1]["parent_id"].as_str(), Some(parent_a));
    assert_eq!(tasks[2]["parent_id"].as_str(), Some(external_parent.as_str()));
}
```

- [ ] **Step 3: Add indentation validation tests**

Append:

```rust
#[test]
fn create_from_file_rejects_tab_indentation() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- Parent\n\t- Child\n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_validation_error(&result);
    assert_eq!(
        result.envelope["error"]["message"].as_str(),
        Some("line 2 tab indentation is not supported")
    );
}

#[test]
fn create_from_file_rejects_odd_indentation() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- Parent\n - Child\n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_validation_error(&result);
    assert_eq!(
        result.envelope["error"]["message"].as_str(),
        Some("line 2 indentation must use multiples of 2 spaces")
    );
}

#[test]
fn create_from_file_rejects_skipped_depth() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- Parent\n    - Grandchild\n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_validation_error(&result);
    assert_eq!(
        result.envelope["error"]["message"].as_str(),
        Some("line 2 indentation jumps from depth 0 to depth 2")
    );
}

#[test]
fn create_from_file_rejects_indented_first_bullet() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "  - Child without parent\n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_validation_error(&result);
    assert_eq!(
        result.envelope["error"]["message"].as_str(),
        Some("line 1 first bullet must not be indented")
    );
}
```

- [ ] **Step 4: Add content validation tests**

Append:

```rust
#[test]
fn create_from_file_rejects_non_bullet_content_with_line_number() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- Parent\nparagraph text\n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_validation_error(&result);
    assert_eq!(
        result.envelope["error"]["message"].as_str(),
        Some("line 2 must be a markdown bullet starting with '- '")
    );
}

#[test]
fn create_from_file_rejects_empty_checkbox_title() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- [ ]   \n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_validation_error(&result);
    assert_eq!(
        result.envelope["error"]["message"].as_str(),
        Some("line 1 task title must not be empty")
    );
}
```

- [ ] **Step 5: Run tests to confirm current failure**

Run:

```bash
cargo test --test verb_first_task_commands create_from_file -- --nocapture
```

Expected before implementation: nested happy-path tests fail because current parser rejects nested bullets; validation messages may fail because old nested message differs.

---

### Task 2: Implement Nested Parser

**Files:**
- Modify: `src/cli/commands/task_create.rs`

- [ ] **Step 1: Add parsed bullet type**

Place above `execute_create`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedTaskBullet {
    title: String,
    depth: usize,
    line_no: usize,
}
```

- [ ] **Step 2: Replace title extraction call**

In `execute_create`, replace:

```rust
let titles = if let Some(path) = args.from_file.as_deref() {
    parse_task_bullets(path)?
} else {
    args.titles
        .iter()
        .map(|value| {
            as_optional_string(Some(value)).ok_or_else(|| {
                TsqError::new("VALIDATION_ERROR", "task title must not be empty", 1)
            })
        })
        .collect::<Result<Vec<_>, _>>()?
};
```

with:

```rust
let parsed_file_tasks = if let Some(path) = args.from_file.as_deref() {
    Some(parse_task_bullets(path)?)
} else {
    None
};
let positional_titles = if parsed_file_tasks.is_none() {
    args.titles
        .iter()
        .map(|value| {
            as_optional_string(Some(value)).ok_or_else(|| {
                TsqError::new("VALIDATION_ERROR", "task title must not be empty", 1)
            })
        })
        .collect::<Result<Vec<_>, _>>()?
} else {
    Vec::new()
};
let create_count = parsed_file_tasks
    .as_ref()
    .map(|items| items.len())
    .unwrap_or(positional_titles.len());
```

- [ ] **Step 3: Update empty/single-only checks**

Replace:

```rust
if titles.is_empty() {
    return Err(TsqError::new(
        "VALIDATION_ERROR",
        "at least one title is required",
        1,
    ));
}
```

with:

```rust
if create_count == 0 {
    return Err(TsqError::new(
        "VALIDATION_ERROR",
        "at least one title is required",
        1,
    ));
}
```

Replace:

```rust
if single_only && titles.len() > 1 {
```

with:

```rust
if single_only && create_count > 1 {
```

- [ ] **Step 4: Replace flat create loop with hierarchy-aware loop**

Replace:

```rust
let mut created = Vec::with_capacity(titles.len());
for (index, title) in titles.into_iter().enumerate() {
    created.push(service.create(CreateInput {
        title,
        kind,
        priority,
        description: description.clone(),
        external_ref: external_ref.clone(),
        discovered_from: discovered_from.clone(),
        parent: args.parent.clone(),
        exact_id: opts.exact_id,
        planning_state,
        explicit_id: if index == 0 {
            explicit_id.clone()
        } else {
            None
        },
        body_file: body_file.clone(),
        ensure: args.ensure,
    })?);
}
```

with:

```rust
let mut created = Vec::with_capacity(create_count);
if let Some(file_tasks) = parsed_file_tasks {
    let mut parent_stack: Vec<String> = Vec::new();
    for item in file_tasks {
        let parent = if item.depth == 0 {
            args.parent.clone()
        } else {
            parent_stack.get(item.depth - 1).cloned().ok_or_else(|| {
                TsqError::new(
                    "VALIDATION_ERROR",
                    format!("line {} has no parent at depth {}", item.line_no, item.depth - 1),
                    1,
                )
            })?
        };
        let task = service.create(CreateInput {
            title: item.title,
            kind,
            priority,
            description: description.clone(),
            external_ref: external_ref.clone(),
            discovered_from: discovered_from.clone(),
            parent,
            exact_id: opts.exact_id,
            planning_state,
            explicit_id: None,
            body_file: body_file.clone(),
            ensure: args.ensure,
        })?;
        parent_stack.truncate(item.depth);
        parent_stack.push(task.id.clone());
        created.push(task);
    }
} else {
    for (index, title) in positional_titles.into_iter().enumerate() {
        created.push(service.create(CreateInput {
            title,
            kind,
            priority,
            description: description.clone(),
            external_ref: external_ref.clone(),
            discovered_from: discovered_from.clone(),
            parent: args.parent.clone(),
            exact_id: opts.exact_id,
            planning_state,
            explicit_id: if index == 0 {
                explicit_id.clone()
            } else {
                None
            },
            body_file: body_file.clone(),
            ensure: args.ensure,
        })?);
    }
}
```

- [ ] **Step 5: Replace parser function**

Replace `fn parse_task_bullets(path: &str) -> Result<Vec<String>, TsqError>` with:

```rust
fn parse_task_bullets(path: &str) -> Result<Vec<ParsedTaskBullet>, TsqError> {
    let content = fs::read_to_string(path).map_err(|error| {
        TsqError::new(
            "IO_ERROR",
            format!("failed reading tasks file: {}", path),
            2,
        )
        .with_details(serde_json::json!({
            "kind": format!("{:?}", error.kind()),
            "message": error.to_string(),
        }))
    })?;
    let mut tasks = Vec::new();
    let mut previous_depth: Option<usize> = None;
    for (index, line) in content.lines().enumerate() {
        let line_no = index + 1;
        if line.trim().is_empty() {
            continue;
        }
        let indent_prefix: String = line
            .chars()
            .take_while(|value| *value == ' ' || *value == '\t')
            .collect();
        if indent_prefix.contains('\t') {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                format!("line {} tab indentation is not supported", line_no),
                1,
            ));
        }

        let indent = indent_prefix.len();
        if indent % 2 != 0 {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                format!("line {} indentation must use multiples of 2 spaces", line_no),
                1,
            ));
        }
        let depth = indent / 2;
        if tasks.is_empty() && depth != 0 {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                format!("line {} first bullet must not be indented", line_no),
                1,
            ));
        }
        if let Some(prev_depth) = previous_depth {
            if depth > prev_depth + 1 {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    format!(
                        "line {} indentation jumps from depth {} to depth {}",
                        line_no, prev_depth, depth
                    ),
                    1,
                ));
            }
        }

        let trimmed = &line[indent..];
        let Some(raw_title) = trimmed.strip_prefix("- ") else {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                format!("line {} must be a markdown bullet starting with '- '", line_no),
                1,
            ));
        };
        let title = raw_title
            .strip_prefix("[ ] ")
            .or_else(|| raw_title.strip_prefix("[x] "))
            .or_else(|| raw_title.strip_prefix("[X] "))
            .unwrap_or(raw_title)
            .trim();
        if title.is_empty() {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                format!("line {} task title must not be empty", line_no),
                1,
            ));
        }
        tasks.push(ParsedTaskBullet {
            title: title.to_string(),
            depth,
            line_no,
        });
        previous_depth = Some(depth);
    }
    if tasks.is_empty() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "tasks file must contain at least one markdown bullet",
            1,
        ));
    }
    Ok(tasks)
}
```

- [ ] **Step 6: Run targeted tests**

Run:

```bash
cargo test --test verb_first_task_commands create_from_file -- --nocapture
```

Expected: all `create_from_file` tests pass.

---

### Task 3: Add Nested `--ensure` Coverage

**Files:**
- Modify: `tests/create_children_batch.rs`

- [ ] **Step 1: Add nested ensure regression test**

Append:

```rust
#[test]
fn create_ensure_is_idempotent_for_nested_file_batch() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- Parent A\n  - Child A1\n- Parent B\n  - Child B1\n").unwrap();
    let cmd = ["create", "--from-file", "tasks.md", "--ensure"];

    let first = run_json(repo.path(), cmd);
    assert_eq!(first.cli.code, 0);
    let second = run_json(repo.path(), cmd);
    assert_eq!(second.cli.code, 0);

    let first_tasks = first.envelope["data"]["tasks"].as_array().expect("first tasks");
    let second_tasks = second.envelope["data"]["tasks"].as_array().expect("second tasks");
    let first_ids: Vec<&str> = first_tasks
        .iter()
        .map(|task| task["id"].as_str().expect("first id"))
        .collect();
    let second_ids: Vec<&str> = second_tasks
        .iter()
        .map(|task| task["id"].as_str().expect("second id"))
        .collect();
    assert_eq!(first_ids, second_ids);

    let listed = run_json(repo.path(), ["find", "open"]);
    assert_eq!(listed.cli.code, 0);
    let all_tasks = listed.envelope["data"]["tasks"].as_array().expect("tasks");
    let matching_count = all_tasks
        .iter()
        .filter(|task| {
            matches!(
                task["title"].as_str(),
                Some("Parent A") | Some("Child A1") | Some("Parent B") | Some("Child B1")
            )
        })
        .count();
    assert_eq!(matching_count, 4);
}
```

- [ ] **Step 2: Add same-title-under-different-parent ensure test**

Append:

```rust
#[test]
fn create_ensure_scopes_nested_titles_to_resolved_parent() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- Parent A\n  - Shared child\n- Parent B\n  - Shared child\n").unwrap();
    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md", "--ensure"]);

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"].as_array().expect("tasks");
    assert_eq!(tasks.len(), 4);
    let parent_a = tasks[0]["id"].as_str().expect("parent A");
    let child_a = tasks[1]["id"].as_str().expect("child A");
    let parent_b = tasks[2]["id"].as_str().expect("parent B");
    let child_b = tasks[3]["id"].as_str().expect("child B");

    assert_eq!(tasks[1]["parent_id"].as_str(), Some(parent_a));
    assert_eq!(tasks[3]["parent_id"].as_str(), Some(parent_b));
    assert_ne!(child_a, child_b);
}
```

- [ ] **Step 3: Run ensure tests**

Run:

```bash
cargo test --test create_children_batch ensure -- --nocapture
```

Expected: nested ensure test passes and existing ensure tests still pass.

---

### Task 4: Update Help And Docs

**Files:**
- Modify: `src/cli/commands/task_create.rs`
- Modify: `README.md`
- Modify: `npm/README.md`
- Modify: `SKILLS/tasque/SKILL.md`
- Modify: `SKILLS/tasque/references/command-reference.md`
- Modify: `SKILLS/tasque/references/task-authoring-checklist.md`
- Modify: `docs/superpowers/specs/2026-05-08-verb-first-cli-ergonomics-design.md`

- [ ] **Step 1: Update create help after_help**

In `src/cli/commands/task_create.rs`, replace the current `tasks.md format` block with:

```text
tasks.md format:
  - Parent task
    - Child task
      - Grandchild task
  - [ ] Checkbox bullet also works
```

- [ ] **Step 2: Update README command section**

In `README.md`, near the `tsq create <title...>` command reference, add this short `tasks.md` example:

```md
- Parent task
  - Child task
    - Grandchild task
- [ ] Another parent task
```

Include command:

```bash
tsq create --from-file tasks.md
tsq create --parent <id> --from-file tasks.md
```

- [ ] **Step 3: Update npm README command section**

In `npm/README.md`, mirror the same `tasks.md` example and commands from README:

```md
- Parent task
  - Child task
    - Grandchild task
- [ ] Another parent task
```

```bash
tsq create --from-file tasks.md
tsq create --parent <id> --from-file tasks.md
```

- [ ] **Step 4: Update Tasque skill main guidance**

In `SKILLS/tasque/SKILL.md`, replace the current `Batch from tasks.md` example with:

```md
- Add parser tests
  - Cover nested task hierarchy
  - Cover invalid indentation
- Wire CLI command
- Update skill docs
```

Keep this command directly under it:

```bash
tsq create --parent <parent-id> --from-file tasks.md
```

- [ ] **Step 5: Update skill references**

In `SKILLS/tasque/references/command-reference.md`, add this under the `create` command:

```md
- Parent task
  - Child task
    - Grandchild task
- [ ] Another parent task
```

In `SKILLS/tasque/references/task-authoring-checklist.md`, add the same canonical block where it describes creating or splitting tasks.

- [ ] **Step 6: Update any remaining exact flat examples**

In each docs/skill file listed above, replace flat-only examples:

```md
- Add parser tests
- Wire CLI command
- Update skill docs
```

with:

```md
- Add parser tests
  - Cover nested task hierarchy
  - Cover invalid indentation
- Wire CLI command
- Update skill docs
```

- [ ] **Step 7: Update old spec language**

In `docs/superpowers/specs/2026-05-08-verb-first-cli-ergonomics-design.md`, replace:

```markdown
- Each top-level `- ` bullet becomes one task title.
...
- Nested bullets are rejected in v1.
- `--parent` makes bullets children.
```

with:

```markdown
- Each `- ` bullet becomes one task title.
- Nested bullets use exactly two spaces per depth.
- Nested bullets create real parent/child hierarchy.
- `--parent` makes top-level bullets children of the given parent.
```

- [ ] **Step 8: Run help/doc smoke**

Run:

```bash
cargo run -q -- create --help | sed -n '1,120p'
```

Expected: help includes nested `tasks.md format` and exits 0.

---

### Task 5: Full Verification And Current-Repo Smoke

**Files:**
- No new code files.

- [ ] **Step 1: Run format**

Run:

```bash
cargo fmt --check
```

Expected: exits 0.

- [ ] **Step 2: Run clippy**

Run:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: exits 0.

- [ ] **Step 3: Run full tests**

Run:

```bash
cargo test --quiet
```

Expected: exits 0.

- [ ] **Step 4: Build release binary**

Run:

```bash
cargo build --release --locked
target/release/tsq --version
```

Expected: binary builds and reports current `Cargo.toml` version.

- [ ] **Step 5: Install compiled binary for current-project smoke**

Run:

```bash
cp target/release/tsq ~/.local/bin/tsq
~/.local/bin/tsq --version
```

Expected: installed binary reports current version.

- [ ] **Step 6: Smoke nested batch create in temporary repo**

Run:

```bash
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
SMOKE_REPO=$(mktemp -d /tmp/tsq-nested-smoke-repo.XXXXXX)
cd "$SMOKE_REPO"
~/.local/bin/tsq init >/tmp/tsq-nested-smoke-init.txt
TASKS_FILE=$(mktemp /tmp/tsq-nested-smoke.XXXXXX)
printf -- '- Nested smoke parent %s\n  - Nested smoke child %s\n    - Nested smoke grandchild %s\n' "$STAMP" "$STAMP" "$STAMP" > "$TASKS_FILE"
~/.local/bin/tsq --format json create --from-file "$TASKS_FILE" --kind task --needs-plan > /tmp/tsq-nested-smoke.json
node -e "const o=JSON.parse(require('fs').readFileSync('/tmp/tsq-nested-smoke.json','utf8')); const t=o.data.tasks; console.log(JSON.stringify(t.map(x=>({id:x.id,title:x.title,parent_id:x.parent_id})), null, 2)); if (t.length!==3 || t[1].parent_id!==t[0].id || t[2].parent_id!==t[1].id) process.exit(1);"
```

Expected: three tasks, child parent is first task, grandchild parent is second task.

- [ ] **Step 7: Run doctor in current repo**

Run:

```bash
~/.local/bin/tsq doctor
```

Expected: `issues=none`.

---

## Plan Self-Review

- Spec coverage: parser, hierarchy mapping, `--parent`, output stability, validation, ensure semantics, docs, tests, and current-repo smoke all mapped to tasks.
- Placeholder scan: no TBD/TODO/fill-in steps.
- Type consistency: parser returns `Vec<ParsedTaskBullet>`; create loop uses `CreateInput` fields already present in `task_create.rs`.
