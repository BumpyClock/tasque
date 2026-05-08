# Verb-First CLI Ergonomics Implementation Plan

> **For agentic workers:** Prefer `subagent-driven-development` for execution when available. Task implementers own task work and review fixes; integration owner owns final integration. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Tasque's nested command grammar with the approved verb-first CLI, update docs/skill guidance, and verify behavior with CLI contract tests.

**Architecture:** Keep the domain/projector/storage model intact. Most changes map new clap command shapes into existing service functions, with small helper additions for spec content reads, Markdown batch-create parsing, and parse-error migration hints. Use focused integration tests as the behavioral contract.

**Tech Stack:** Rust, clap, serde/serde_json, existing Tasque service layer, Cargo integration tests.

---

## File Ownership Map

- `src/cli/program.rs`: root command enum, global output args, parse-error migration hints, dispatch.
- `src/cli/action.rs`: `GlobalOpts` format representation and JSON/human output branching.
- `src/cli/commands/task.rs`: `create`, `edit`, `find`, lifecycle verbs, duplicate/supersede grammar, merge/duplicates/stale/history kept commands.
- `src/cli/commands/note.rs`: new `note`/`notes` execution surface.
- `src/cli/commands/spec.rs`: new `spec <id> --text|--file|--stdin|--show|--check` surface.
- `src/cli/commands/dep.rs`: new `block`, `unblock`, `order`, `unorder`, `deps`.
- `src/cli/commands/link.rs`: new `relate`, `unrelate`.
- `src/cli/commands/label.rs`: new `label`, `unlabel`, `labels`.
- `src/app/service_types.rs`: result/input structs for spec content if needed.
- `src/app/service_specs.rs`: service helper to read resolved spec content.
- `src/cli/render.rs`: human delimiters for spec content.
- `tests/verb_first_*.rs`: new CLI contract coverage.
- `tests/common/mod.rs`: helper updates from old command names to new command names.
- `README.md`, `npm/README.md`, `AGENTS-reference.md`, `docs/planning-workflow.md`, `SKILLS/tasque/**`, `/Users/adityasharma/Projects/dotfiles/skills/tasque/**`: docs and skill updates.

## Task 1: CLI Root, Output Format, Migration Hints

**Files:**
- Modify: `src/cli/program.rs`
- Modify: `src/cli/action.rs`
- Test: `tests/verb_first_output_errors.rs`

- [ ] **Step 1: Write failing tests for global format and old-command hints**

Add `tests/verb_first_output_errors.rs`:

```rust
mod common;

use common::{create_task, init_repo, run_cli, run_json_explicit};
use serde_json::Value;

#[test]
fn format_json_outputs_json_envelope() {
    let repo = common::make_repo();
    init_repo(repo.path());
    create_task(repo.path(), "Format json target");

    let result = run_json_explicit(repo.path(), ["--format", "json", "find", "open"]);

    assert_eq!(result.cli.code, 0);
    assert_eq!(result.envelope.get("ok").and_then(Value::as_bool), Some(true));
    assert_eq!(
        result.envelope.get("command").and_then(Value::as_str),
        Some("tsq find open")
    );
}

#[test]
fn json_conflicts_with_format_human() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_json_explicit(repo.path(), ["--json", "--format", "human", "find", "open"]);

    assert_eq!(result.cli.code, 1);
    assert_eq!(result.envelope.get("ok").and_then(Value::as_bool), Some(false));
    assert_eq!(
        result
            .envelope
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(Value::as_str),
        Some("VALIDATION_ERROR")
    );
}

#[test]
fn removed_note_add_points_to_note_command() {
    let repo = common::make_repo();
    init_repo(repo.path());

    let result = run_cli(repo.path(), ["note", "add", "tsq-aaaaaaaa", "text"]);

    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("tsq note <id> \"text\""),
        "stderr:\n{}",
        result.stderr
    );
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cargo test --test verb_first_output_errors --quiet
```

Expected: fails because `--format`, `find`, and removed-command hint behavior are not implemented.

- [ ] **Step 3: Implement format opts and parse-error mapping**

Change `GlobalOpts` to represent format while preserving `opts.json` helper behavior:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug, Clone, Copy)]
pub struct GlobalOpts {
    pub format: OutputFormat,
    pub exact_id: bool,
}

impl GlobalOpts {
    pub fn json(self) -> bool {
        self.format == OutputFormat::Json
    }
}
```

Update `run_action` and `emit_error` to use `opts.json()`.

In `src/cli/program.rs`, add:

```rust
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum FormatArg {
    Human,
    Json,
}
```

Add global args:

```rust
#[arg(long, global = true)]
pub json: bool,
#[arg(long, global = true, value_enum)]
pub format: Option<FormatArg>,
```

Convert args:

```rust
fn global_opts(json: bool, format: Option<FormatArg>, exact_id: bool) -> Result<GlobalOpts, TsqError> {
    if json && matches!(format, Some(FormatArg::Human)) {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --json with --format human",
            1,
        ));
    }
    let format = if json || matches!(format, Some(FormatArg::Json)) {
        OutputFormat::Json
    } else {
        OutputFormat::Human
    };
    Ok(GlobalOpts { format, exact_id })
}
```

Add a pre-parse migration mapper that sees old roots/subcommands in `std::env::args_os()` and emits `TsqError` via `emit_error`:

```rust
fn removed_command_hint(args: &[String]) -> Option<&'static str> {
    match args.get(1).map(String::as_str) {
        Some("list") => Some("use `tsq find open` or `tsq find <status>`"),
        Some("ready") => Some("use `tsq find ready --lane coding`"),
        Some("search") => Some("use `tsq find search \"query\"`"),
        Some("update") => Some("use `tsq edit <id> ...` or lifecycle verbs like `tsq done <id>`"),
        Some("close") => Some("use `tsq done <id>`"),
        Some("dep") => Some("use `tsq block <task> by <blocker>` or `tsq order <later> after <earlier>`"),
        Some("link") => Some("use `tsq relate <a> <b>`"),
        Some("note") if args.get(2).map(String::as_str) == Some("add") => {
            Some("use `tsq note <id> \"text\"`")
        }
        Some("note") if args.get(2).map(String::as_str) == Some("list") => {
            Some("use `tsq notes <id>`")
        }
        Some("spec") if args.get(2).map(String::as_str) == Some("attach") => {
            Some("use `tsq spec <id> --file spec.md` or `tsq spec <id> --text \"...\"`")
        }
        Some("spec") if args.get(2).map(String::as_str) == Some("check") => {
            Some("use `tsq spec <id> --check`")
        }
        _ => None,
    }
}
```

- [ ] **Step 4: Verify task tests**

Run:

```bash
cargo test --test verb_first_output_errors --quiet
```

Expected: pass.

## Task 2: Create, Find, Edit, Lifecycle Task Verbs

**Files:**
- Modify: `src/cli/commands/task.rs`
- Modify: `src/cli/program.rs`
- Test: `tests/verb_first_task_commands.rs`

- [ ] **Step 1: Write failing task command tests**

Add tests covering:

```rust
mod common;

use common::{create_task, init_repo, run_json};
use serde_json::Value;

#[test]
fn create_accepts_variadic_children_under_parent() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let parent = create_task(repo.path(), "Parent");

    let result = run_json(repo.path(), ["create", "--parent", &parent, "A", "B"]);

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"].as_array().expect("tasks");
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["parent_id"].as_str(), Some(parent.as_str()));
}

#[test]
fn create_from_file_accepts_markdown_bullets() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let file = repo.path().join("tasks.md");
    std::fs::write(&file, "- [ ] Add parser tests\n- Wire CLI command\n").unwrap();

    let result = run_json(repo.path(), ["create", "--from-file", "tasks.md"]);

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"].as_array().expect("tasks");
    assert_eq!(tasks[0]["title"].as_str(), Some("Add parser tests"));
    assert_eq!(tasks[1]["title"].as_str(), Some("Wire CLI command"));
}

#[test]
fn edit_updates_metadata_without_status_change() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Old title");

    let result = run_json(repo.path(), ["edit", &id, "--title", "New title", "--priority", "1"]);

    assert_eq!(result.cli.code, 0);
    assert_eq!(result.envelope["data"]["task"]["title"].as_str(), Some("New title"));
    assert_eq!(result.envelope["data"]["task"]["priority"].as_u64(), Some(1));
}

#[test]
fn lifecycle_done_accepts_note_and_multiple_ids() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let a = create_task(repo.path(), "A");
    let b = create_task(repo.path(), "B");

    let result = run_json(repo.path(), ["done", &a, &b, "--note", "verified"]);

    assert_eq!(result.cli.code, 0);
    let tasks = result.envelope["data"]["tasks"].as_array().expect("tasks");
    assert_eq!(tasks.len(), 2);
    assert!(tasks.iter().all(|task| task["status"].as_str() == Some("closed")));
}

#[test]
fn find_search_replaces_search() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Needle task");
    create_task(repo.path(), "Other task");

    let result = run_json(repo.path(), ["find", "search", "Needle"]);

    assert_eq!(result.cli.code, 0);
    let ids: Vec<&str> = result.envelope["data"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|task| task["id"].as_str())
        .collect();
    assert_eq!(ids, vec![id.as_str()]);
}
```

- [ ] **Step 2: Implement task args and execution**

Refactor `CreateArgs` to:

```rust
pub struct CreateArgs {
    pub titles: Vec<String>,
    #[arg(long = "from-file")]
    pub from_file: Option<String>,
    #[arg(long = "planned", default_value_t = false)]
    pub planned: bool,
    #[arg(long = "needs-plan", default_value_t = false)]
    pub needs_plan: bool,
    // keep kind, priority, parent, description, external-ref, discovered-from, id, body-file, ensure
}
```

Add `parse_task_bullets(path: &str) -> Result<Vec<String>, TsqError>` in `task.rs` or a small helper module.

Add new args/execute fns:

- `EditArgs` -> existing `UpdateInput` without status/planning.
- `FindArgs` with subcommand or enum for `ready`, status buckets, and `search`.
- `ClaimArgs`, `AssignArgs`, `StartArgs`, `OpenArgs`, `BlockedArgs`, `PlannedArgs`, `NeedsPlanArgs`, `DeferArgs`, `DoneArgs`, `CancelArgs`.

Map lifecycle fns to existing `service.claim`, `service.update`, `service.close`, `service.reopen`, and `service.note_add` for `--note`.

- [ ] **Step 3: Verify task command tests**

Run:

```bash
cargo test --test verb_first_task_commands --quiet
```

Expected: pass.

## Task 3: Note And Spec Verbs With Spec Content Read

**Files:**
- Modify: `src/cli/commands/note.rs`
- Modify: `src/cli/commands/spec.rs`
- Modify: `src/app/service_types.rs`
- Modify: `src/app/service_specs.rs`
- Modify: `src/cli/render.rs`
- Test: `tests/verb_first_note_spec.rs`

- [ ] **Step 1: Write failing note/spec tests**

Cover:

- `tsq note <id> "text"` adds note.
- `tsq notes <id>` lists notes.
- `tsq note <id> --stdin` rejects empty stdin.
- `tsq spec <id> --text "..."` attaches spec.
- `tsq spec <id> --show` prints markdown in human output.
- `tsq --format json spec <id> --show` includes `data.spec.content`.
- `tsq show <id> --with-spec` includes content only when requested.

- [ ] **Step 2: Add spec content result type**

Add to `service_types.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecContentInput {
    pub id: String,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecContentResult {
    pub task_id: String,
    pub spec_path: String,
    pub spec_fingerprint: String,
    pub content: String,
}
```

- [ ] **Step 3: Implement service helper**

In `service_specs.rs`, add:

```rust
pub fn spec_content(
    ctx: &ServiceContext,
    input: &SpecContentInput,
) -> Result<SpecContentResult, TsqError> {
    let loaded = load_projected_state(&ctx.repo_root)?;
    let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;
    let task = must_task(&loaded.state, &id)?;
    let spec_path = task.spec_path.clone().ok_or_else(|| {
        TsqError::new("VALIDATION_ERROR", format!("task {} has no attached spec; use `tsq spec {} --file spec.md`", id, id), 1)
    })?;
    let spec_fingerprint = task.spec_fingerprint.clone().ok_or_else(|| {
        TsqError::new("VALIDATION_ERROR", format!("task {} has no attached spec fingerprint", id), 1)
    })?;
    let content = std::fs::read_to_string(ctx.repo_root.join(&spec_path)).map_err(|error| {
        TsqError::new("IO_ERROR", format!("failed reading spec {}", spec_path), 2)
            .with_details(serde_json::json!({ "message": error.to_string() }))
    })?;
    Ok(SpecContentResult { task_id: id, spec_path, spec_fingerprint, content })
}
```

Adjust root resolution if current service context already points at sync worktree after `find_tasque_root`.

- [ ] **Step 4: Implement CLI execution**

Map:

- `note <id> <text>` -> `NoteAddInput`.
- `note <id> --stdin` -> read `crate::app::stdin::read_stdin_content`.
- `notes <id>` -> `NoteListInput`.
- `spec <id> --text|--file|--stdin --force` -> existing `SpecAttachInput`.
- `spec <id> --check` -> existing `SpecCheckInput`.
- `spec <id> --show` -> new spec content helper.

- [ ] **Step 5: Verify note/spec tests**

Run:

```bash
cargo test --test verb_first_note_spec --quiet
```

Expected: pass.

## Task 4: Relation, Label, Dependency Verbs

**Files:**
- Modify: `src/cli/commands/dep.rs`
- Modify: `src/cli/commands/link.rs`
- Modify: `src/cli/commands/label.rs`
- Modify: `src/cli/program.rs`
- Test: `tests/verb_first_relations.rs`

- [ ] **Step 1: Write failing relation tests**

Cover:

- `tsq block <task> by <blocker>`
- `tsq unblock <task> by <blocker>`
- `tsq order <later> after <earlier>`
- `tsq unorder <later> after <earlier>`
- `tsq relate <a> <b>`
- `tsq unrelate <a> <b>`
- `tsq label <id> <label>`
- `tsq unlabel <id> <label>`
- malformed token errors, e.g. `tsq block <task> from <blocker>`

- [ ] **Step 2: Implement sentence token args**

Use explicit positional token fields:

```rust
pub struct BlockArgs {
    pub child: String,
    pub by: String,
    pub blocker: String,
}
```

Validate `by == "by"` and `after == "after"` in execution, returning `VALIDATION_ERROR` with corrected command examples.

Map:

- `block/unblock` -> `DepInput { dep_type: Some(Blocks) }`
- `order/unorder` -> `DepInput { dep_type: Some(StartsAfter) }`
- `relate/unrelate` -> `LinkInput { rel_type: RelatesTo }`
- `label/unlabel/labels` -> existing label service.
- `deps` -> existing dep tree service.

- [ ] **Step 3: Verify relation tests**

Run:

```bash
cargo test --test verb_first_relations --quiet
```

Expected: pass.

## Task 5: Docs, Help, Embedded Skill

**Files:**
- Modify: `README.md`
- Modify: `npm/README.md`
- Modify: `AGENTS-reference.md`
- Modify: `docs/planning-workflow.md`
- Modify: `SKILLS/tasque/SKILL.md`
- Modify: `SKILLS/tasque/references/*.md`
- Modify: `/Users/adityasharma/Projects/dotfiles/skills/tasque/SKILL.md`
- Modify: `/Users/adityasharma/Projects/dotfiles/skills/tasque/references/*.md`
- Test: `tests/embedded_skills.rs`

- [ ] **Step 1: Update docs to verb-first commands**

Replace old examples:

- `tsq ready --lane coding` -> `tsq find ready --lane coding`
- `tsq list --status blocked` -> `tsq find blocked`
- `tsq create --parent <id> --child ...` -> `tsq create --parent <id> "Child A" "Child B"`
- `tsq spec attach` -> `tsq spec`
- `tsq update --planning planned` -> `tsq planned`
- `tsq update --status closed` -> `tsq done`
- `tsq note add` -> `tsq note`

- [ ] **Step 2: Add batch create guidance to skill**

Add exact `tasks.md` format:

```md
- Add parser tests
- Wire CLI command
- Update skill docs
```

Guidance:

```text
Use `tsq create --parent <id> --from-file tasks.md` for many tasks.
Use `--format json` only when scripting/parsing; human output is fine for inspection.
Use `tsq spec <id> --show` when you need spec markdown from sync worktree.
```

- [ ] **Step 3: Add embedded skill assertion**

Extend `tests/embedded_skills.rs` to read installed `SKILL.md` and assert it contains:

```rust
assert!(contents.contains("tsq find ready --lane coding"));
assert!(contents.contains("tsq create --parent <parent-id> --from-file tasks.md"));
assert!(contents.contains("tsq spec <id> --show"));
```

- [ ] **Step 4: Verify docs/skill test**

Run:

```bash
cargo test --test embedded_skills --quiet
```

Expected: pass.

## Task 6: Integration Cleanup And Full Gate

**Files:**
- Modify only files needed to resolve cross-task compiler/test failures.

- [ ] **Step 1: Run focused changed-test suite**

Run:

```bash
cargo test --test verb_first_output_errors --test verb_first_task_commands --test verb_first_note_spec --test verb_first_relations --test embedded_skills --quiet
```

Expected: pass.

- [ ] **Step 2: Run full gate**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --quiet
```

Expected: all pass. If any fail, fix the root cause and rerun the failing command.

- [ ] **Step 3: Build and install local binary**

Run:

```bash
cargo build --release --bin tsq
cp target/release/tsq ~/.local/bin/tsq
tsq --version
```

Expected: `tsq` resolves to updated release binary from `~/.local/bin`.

