# Durability Correctness Implementation Plan

> **For agentic workers:** Prefer `subagent-driven-development` for execution when available. Task implementers own task work and review fixes; integration owner owns final integration. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden Tasque append/replay, projected-state shortcuts, lock behavior, and `find --full` docs before library-facade work.

**Architecture:** Keep event log as source of truth. Validate records before append and before replay with the same private validators. Treat state cache and snapshots as rebuildable accelerators: invalid shortcut state becomes a miss, not a user-visible failure.

**Tech Stack:** Rust, serde/serde_json, JSONL files under `.tasque`, tempfile-based tests, existing `TsqError`.

---

## File Structure

- Modify `src/store/events.rs`: shared event validation, corrupt-tail-safe append, append tests support.
- Modify `tests/event_read_validation.rs`: event read/write boundary regression tests.
- Create `src/domain/state_invariants.rs`: projected `State` invariant validator.
- Modify `src/domain/mod.rs`: expose invariant module internally.
- Modify `src/store/state.rs`: ignore invalid cache state.
- Modify `src/store/snapshots.rs`: skip invalid snapshot state through existing warning path.
- Modify `src/app/state.rs`: cache/snapshot fallback tests.
- Modify `src/store/lock.rs`: lock timeout helper, ownership mismatch error, lock tests.
- Modify `README.md`, `npm/README.md`, `AGENTS-reference.md`, `SKILLS/tasque/references/command-reference.md`: `find --full` wording.

## Subagent Split

- Worker A owns event append/read validation: `src/store/events.rs`, `tests/event_read_validation.rs`.
- Worker B owns state shortcut validation: `src/domain/state_invariants.rs`, `src/domain/mod.rs`, `src/store/state.rs`, `src/store/snapshots.rs`, `src/app/state.rs`.
- Worker C owns lock behavior/tests: `src/store/lock.rs`.
- Worker D owns docs drift: `README.md`, `npm/README.md`, `AGENTS-reference.md`, `SKILLS/tasque/references/command-reference.md`.
- Integration owner runs format, tests, and resolves conflicts only after workers finish.

### Task 1: Event Validation And Corrupt Tail Append

**Files:**
- Modify: `src/store/events.rs`
- Modify: `tests/event_read_validation.rs`

- [ ] **Step 1: Add failing tests for corrupt tail append and outbound validation**

Replace the import block in `tests/event_read_validation.rs` with:

```rust
use tasque::app::state::load_projected_state;
use tasque::domain::events::make_event;
use tasque::store::events::{append_events, read_events_from_path};
use tasque::types::EventType;
use serde_json::json;
use std::fs;
use tempfile::TempDir;
```

Add helper:

```rust
fn task_created_event(task_id: &str, title: &str) -> tasque::types::EventRecord {
    let mut payload = serde_json::Map::new();
    payload.insert("title".to_string(), json!(title));
    make_event(
        "test",
        "2026-04-21T00:00:00.000Z",
        EventType::TaskCreated,
        task_id,
        payload,
    )
}
```

Add tests:

```rust
#[test]
fn append_trims_malformed_trailing_jsonl_line_before_writing_new_events() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let paths = tasque::store::paths::get_paths(repo);
    std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");

    let first = task_created_event("tsq-root0001", "first");
    let first_line = serde_json::to_string(&first).expect("serialize first");
    fs::write(&paths.events_file, format!("{}\n{{", first_line)).expect("write corrupt tail");

    append_events(repo, &[task_created_event("tsq-root0002", "second")]).expect("append after corrupt tail");

    let raw = fs::read_to_string(&paths.events_file).expect("read events");
    assert!(raw.lines().all(|line| line.trim() != "{"));
    let read = read_events_from_path(&paths.events_file).expect("read repaired events");
    let ids = read.events.iter().map(|event| event.task_id.as_str()).collect::<Vec<_>>();
    assert_eq!(ids, vec!["tsq-root0001", "tsq-root0002"]);
    assert!(read.warning.is_none());
}

#[test]
fn append_trims_malformed_trailing_jsonl_line_even_when_file_ends_with_newline() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let paths = tasque::store::paths::get_paths(repo);
    std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");

    let first = task_created_event("tsq-root0001", "first");
    let first_line = serde_json::to_string(&first).expect("serialize first");
    fs::write(&paths.events_file, format!("{}\n{{\n", first_line)).expect("write corrupt tail");

    append_events(repo, &[task_created_event("tsq-root0002", "second")]).expect("append after corrupt tail");

    let raw = fs::read_to_string(&paths.events_file).expect("read events");
    assert!(raw.lines().all(|line| line.trim() != "{"));
    let read = read_events_from_path(&paths.events_file).expect("read repaired events");
    let ids = read.events.iter().map(|event| event.task_id.as_str()).collect::<Vec<_>>();
    assert_eq!(ids, vec!["tsq-root0001", "tsq-root0002"]);
    assert!(read.warning.is_none());
}

#[test]
fn append_after_malformed_tail_survives_app_level_replay_without_cache() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let paths = tasque::store::paths::get_paths(repo);
    std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");

    let first = task_created_event("tsq-root0001", "first");
    let first_line = serde_json::to_string(&first).expect("serialize first");
    fs::write(&paths.events_file, format!("{}\n{{", first_line)).expect("write corrupt tail");

    append_events(repo, &[task_created_event("tsq-root0002", "second")]).expect("append second");
    let _ = fs::remove_file(&paths.state_file);

    let loaded = load_projected_state(repo).expect("full replay");
    assert!(loaded.state.tasks.contains_key("tsq-root0001"));
    assert!(loaded.state.tasks.contains_key("tsq-root0002"));
}

#[test]
fn append_adds_separator_after_valid_final_line_without_newline() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let paths = tasque::store::paths::get_paths(repo);
    std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");

    let first = task_created_event("tsq-root0001", "first");
    fs::write(&paths.events_file, serde_json::to_string(&first).expect("serialize first"))
        .expect("write final line without newline");

    append_events(repo, &[task_created_event("tsq-root0002", "second")]).expect("append with separator");

    let read = read_events_from_path(&paths.events_file).expect("read events");
    assert_eq!(read.events.len(), 2);
}

#[test]
fn append_rejects_invalid_outbound_event_without_changing_file() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let paths = tasque::store::paths::get_paths(repo);
    append_events(repo, &[task_created_event("tsq-root0001", "first")]).expect("append first");
    let before = fs::read(&paths.events_file).expect("read before");

    let mut bad = task_created_event("tsq-root0002", "second");
    bad.payload.remove("title");

    let err = append_events(repo, &[bad]).expect_err("invalid append should fail");
    assert_eq!(err.code, "EVENT_APPEND_FAILED");
    assert_eq!(fs::read(&paths.events_file).expect("read after"), before);
}

#[test]
fn append_rejects_missing_link_target_missing_supersede_with_and_invalid_priority() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();

    let mut link_payload = serde_json::Map::new();
    link_payload.insert("type".to_string(), json!("relates_to"));
    let missing_target = make_event(
        "test",
        "2026-04-21T00:00:00.000Z",
        EventType::LinkAdded,
        "tsq-root0001",
        link_payload,
    );
    assert_eq!(append_events(repo, &[missing_target]).expect_err("missing target").code, "EVENT_APPEND_FAILED");

    let missing_with = make_event(
        "test",
        "2026-04-21T00:00:00.000Z",
        EventType::TaskSuperseded,
        "tsq-root0001",
        serde_json::Map::new(),
    );
    assert_eq!(append_events(repo, &[missing_with]).expect_err("missing with").code, "EVENT_APPEND_FAILED");

    let mut bad_priority = task_created_event("tsq-root0003", "priority");
    bad_priority.payload.insert("priority".to_string(), json!(9));
    assert_eq!(append_events(repo, &[bad_priority]).expect_err("bad priority").code, "EVENT_APPEND_FAILED");
}

#[test]
fn append_rejects_empty_required_payload_string() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();

    let empty_title = task_created_event("tsq-root0001", "");

    let err = append_events(repo, &[empty_title]).expect_err("empty title should fail");
    assert_eq!(err.code, "EVENT_APPEND_FAILED");
}

#[test]
fn append_fails_on_malformed_non_final_jsonl_line_without_changing_file() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let paths = tasque::store::paths::get_paths(repo);
    std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");

    let valid_final = serde_json::to_string(&task_created_event("tsq-root0001", "first"))
        .expect("serialize final");
    fs::write(&paths.events_file, format!("{{\n{}\n", valid_final)).expect("write malformed earlier line");
    let before = fs::read(&paths.events_file).expect("read before");

    let err = append_events(repo, &[task_created_event("tsq-root0002", "second")])
        .expect_err("malformed earlier line should fail");

    assert_eq!(err.code, "EVENTS_CORRUPT");
    assert_eq!(fs::read(&paths.events_file).expect("read after"), before);
}
```

- [ ] **Step 2: Run tests to verify failures**

Run: `cargo test --test event_read_validation`

Expected before implementation: at least one test fails because append does not trim malformed tail, does not add separator after valid no-newline tail, or does not validate outbound payload before write.

- [ ] **Step 3: Extend payload validation**

In `src/store/events.rs`, update `required_fields`:

```rust
EventType::TaskSuperseded => &[("with", "string")],
EventType::LinkAdded => &[("type", "string"), ("target", "string")],
EventType::LinkRemoved => &[("type", "string"), ("target", "string")],
```

Change required-field validation so required strings must be nonempty:

```rust
let type_mismatch = match *expected {
    "string" => value
        .and_then(Value::as_str)
        .filter(|raw| !raw.is_empty())
        .is_none(),
    _ => true,
};
```

Add helpers near `validate_optional_enum_field`:

```rust
fn validate_optional_priority(
    event_type: &EventType,
    payload: &Map<String, Value>,
    line: usize,
) -> Result<(), TsqError> {
    if let Some(value) = payload.get("priority") {
        let Some(priority) = value.as_u64() else {
            return Err(invalid_event_payload_field(event_type, "priority", line, "must be an integer 0..=3"));
        };
        if priority > 3 {
            return Err(invalid_event_payload_field(event_type, "priority", line, "must be an integer 0..=3"));
        }
    }
    Ok(())
}

fn validate_optional_labels(
    event_type: &EventType,
    payload: &Map<String, Value>,
    line: usize,
) -> Result<(), TsqError> {
    if let Some(value) = payload.get("labels") {
        let Some(labels) = value.as_array() else {
            return Err(invalid_event_payload_field(event_type, "labels", line, "must be an array of strings"));
        };
        if labels.iter().any(|label| label.as_str().is_none()) {
            return Err(invalid_event_payload_field(event_type, "labels", line, "must be an array of strings"));
        }
    }
    Ok(())
}

fn validate_optional_bool(
    event_type: &EventType,
    payload: &Map<String, Value>,
    field: &'static str,
    line: usize,
) -> Result<(), TsqError> {
    if let Some(value) = payload.get(field)
        && value.as_bool().is_none()
    {
        return Err(invalid_event_payload_field(event_type, field, line, "must be a boolean"));
    }
    Ok(())
}

fn validate_optional_nonempty_string(
    event_type: &EventType,
    payload: &Map<String, Value>,
    field: &'static str,
    line: usize,
) -> Result<(), TsqError> {
    if let Some(value) = payload.get(field)
        && value.as_str().filter(|raw| !raw.is_empty()).is_none()
    {
        return Err(invalid_event_payload_field(event_type, field, line, "must be a nonempty string"));
    }
    Ok(())
}
```

Call them from `validate_event_payload`:

```rust
validate_optional_priority(event_type, payload, line)?;
validate_optional_labels(event_type, payload, line)?;
for field in ["clear_description", "clear_external_ref", "clear_discovered_from"] {
    validate_optional_bool(event_type, payload, field, line)?;
}
for field in [
    "parent_id",
    "superseded_by",
    "duplicate_of",
    "replies_to",
    "discovered_from",
    "with",
    "blocker",
    "target",
] {
    validate_optional_nonempty_string(event_type, payload, field, line)?;
}
```

- [ ] **Step 4: Validate outbound events before opening file**

Add private helper:

```rust
fn validate_event_for_append(event: &EventRecord) -> Result<(), TsqError> {
    let value = serde_json::to_value(event).map_err(|error| {
        TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
            .with_details(any_error_value(&error))
    })?;
    parse_event_record(&value, 0).map(|_| ()).map_err(|error| {
        TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
            .with_details(serde_json::json!({
                "validation_code": error.code,
                "message": error.message,
            }))
    })
}
```

Call this at top of `append_events` after empty slice guard and before `create_dir_all`:

```rust
for event in events {
    validate_event_for_append(event)?;
}
```

- [ ] **Step 5: Make append repair trailing malformed line and preserve valid no-newline lines**

Update imports:

```rust
use std::fs::{OpenOptions, create_dir_all, read, read_to_string};
use std::io::{Seek, SeekFrom, Write};
```

Add helper:

```rust
fn prepare_event_file_for_append(handle: &mut std::fs::File, path: &Path) -> Result<bool, TsqError> {
    let bytes = read(path).map_err(|error| {
        TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
            .with_details(io_error_value(&error))
    })?;
    if bytes.is_empty() {
        handle.seek(SeekFrom::End(0)).map_err(|error| {
            TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                .with_details(io_error_value(&error))
        })?;
        return Ok(false);
    }

    let raw = std::str::from_utf8(&bytes).map_err(|error| {
        TsqError::new("EVENTS_CORRUPT", "Events file is not valid UTF-8", 2)
            .with_details(any_error_value(&error))
    })?;
    let mut nonempty_lines: Vec<(usize, &str, usize)> = Vec::new();
    let mut offset = 0;
    for (line_index, raw_line) in raw.split_inclusive('\n').enumerate() {
        let start = offset;
        offset += raw_line.len();
        let line = raw_line
            .strip_suffix('\n')
            .unwrap_or(raw_line)
            .trim_end_matches('\r');
        if !line.trim().is_empty() {
            nonempty_lines.push((start, line, line_index + 1));
        }
    }
    if offset < raw.len() {
        let line = raw[offset..].trim_end_matches('\r');
        if !line.trim().is_empty() {
            nonempty_lines.push((offset, line, raw[..offset].matches('\n').count() + 1));
        }
    }

    let Some(final_index) = nonempty_lines.len().checked_sub(1) else {
        handle.seek(SeekFrom::End(0)).map_err(|error| {
            TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                .with_details(io_error_value(&error))
        })?;
        return Ok(false);
    };

    for (index, (_start, line, line_number)) in nonempty_lines.iter().enumerate() {
        match serde_json::from_str::<Value>(line) {
            Ok(_) => {}
            Err(_) if index == final_index => {}
            Err(_) => {
                return Err(TsqError::new(
                    "EVENTS_CORRUPT",
                    format!("Malformed events JSONL at line {}", line_number),
                    2,
                ));
            }
        }
    }

    let (last_start, final_line, _line_number) = nonempty_lines[final_index];
    match serde_json::from_str::<Value>(final_line) {
        Ok(_) => {
            handle.seek(SeekFrom::End(0)).map_err(|error| {
                TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                    .with_details(io_error_value(&error))
            })?;
            Ok(!bytes.ends_with(b"\n"))
        }
        Err(_) => {
            handle.set_len(last_start as u64).map_err(|error| {
                TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                    .with_details(io_error_value(&error))
            })?;
            handle.seek(SeekFrom::End(0)).map_err(|error| {
                TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
                    .with_details(io_error_value(&error))
            })?;
            Ok(false)
        }
    }
}
```

Change `OpenOptions` in `append_events`:

```rust
let mut handle = OpenOptions::new()
    .read(true)
    .write(true)
    .create(true)
    .open(&paths.events_file)
    .map_err(|error| {
        TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
            .with_details(io_error_value(&error))
    })?;
let needs_separator = prepare_event_file_for_append(&mut handle, &paths.events_file)?;
if needs_separator {
    handle.write_all(b"\n").map_err(|error| {
        TsqError::new("EVENT_APPEND_FAILED", "Failed appending events", 2)
            .with_details(io_error_value(&error))
    })?;
}
```

- [ ] **Step 6: Verify event task**

Run: `cargo test --test event_read_validation`

Expected: all event validation tests pass.

### Task 2: Projected State Shortcut Validation

**Files:**
- Create: `src/domain/state_invariants.rs`
- Modify: `src/domain/mod.rs`
- Modify: `src/store/state.rs`
- Modify: `src/store/snapshots.rs`
- Modify: `src/app/state.rs`

- [ ] **Step 1: Add failing cache and snapshot fallback tests**

In `src/app/state.rs` test module, replace the existing `crate::types` import with:

```rust
use crate::types::{EventType, SCHEMA_VERSION, Snapshot, StateCache, TaskStatus, STATE_CACHE_SCHEMA_VERSION};
```

Keep the existing `use super::*;`, `make_event`, store imports, `serde_json::Map`, and `tempfile::TempDir` imports unchanged.

Add tests:

```rust
#[test]
fn invalid_priority_state_cache_is_ignored_and_replayed() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let task_id = "tsq-aaaaaaaa";
    append_events(repo, &[created_event(task_id, "first")]).expect("append events");

    let mut state = load_projected_state(repo).expect("load").state;
    state.tasks.get_mut(task_id).expect("task").priority = 9;
    state.applied_events = 1;
    let metadata = read_event_log_metadata(repo, 1).expect("metadata");
    let cache = StateCache {
        schema_version: STATE_CACHE_SCHEMA_VERSION,
        event_log: Some(metadata),
        state,
    };
    let paths = get_paths(repo);
    std::fs::write(&paths.state_file, serde_json::to_string_pretty(&cache).expect("serialize cache"))
        .expect("write invalid cache");

    let reloaded = load_projected_state(repo).expect("reload");

    assert_eq!(reloaded.state.tasks.get(task_id).expect("task").priority, 0);
}

#[test]
fn bad_ref_state_cache_is_ignored_and_replayed() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let task_id = "tsq-aaaaaaaa";
    append_events(repo, &[created_event(task_id, "first")]).expect("append events");

    let mut state = load_projected_state(repo).expect("load").state;
    state.tasks.get_mut(task_id).expect("task").parent_id = Some("tsq-missing".to_string());
    state.applied_events = 1;
    let metadata = read_event_log_metadata(repo, 1).expect("metadata");
    let cache = StateCache {
        schema_version: STATE_CACHE_SCHEMA_VERSION,
        event_log: Some(metadata),
        state,
    };
    let paths = get_paths(repo);
    std::fs::write(&paths.state_file, serde_json::to_string_pretty(&cache).expect("serialize cache"))
        .expect("write invalid cache");

    let reloaded = load_projected_state(repo).expect("reload");

    assert!(reloaded.state.tasks.get(task_id).expect("task").parent_id.is_none());
}

#[test]
fn invalid_primary_state_cache_does_not_fall_back_to_stale_legacy_cache() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    append_events(repo, &[created_event("tsq-aaaaaaaa", "first")]).expect("append first");
    let mut loaded = load_projected_state(repo).expect("load").state;
    persist_projection(repo, &mut loaded, 1, None).expect("persist primary cache");

    append_events(repo, &[created_event("tsq-bbbbbbbb", "second")]).expect("append second");
    let paths = get_paths(repo);
    std::fs::write(
        paths.tasque_dir.join("tasks.jsonl"),
        serde_json::to_string_pretty(&loaded).expect("serialize stale legacy"),
    )
    .expect("write stale legacy cache");

    let mut invalid_primary = loaded.clone();
    invalid_primary.tasks.get_mut("tsq-aaaaaaaa").expect("task").priority = 9;
    invalid_primary.applied_events = 2;
    let metadata = read_event_log_metadata(repo, 2).expect("metadata");
    let cache = StateCache {
        schema_version: STATE_CACHE_SCHEMA_VERSION,
        event_log: Some(metadata),
        state: invalid_primary,
    };
    std::fs::write(&paths.state_file, serde_json::to_string_pretty(&cache).expect("serialize cache"))
        .expect("write invalid primary cache");

    let reloaded = load_projected_state(repo).expect("reload");

    assert!(reloaded.state.tasks.contains_key("tsq-aaaaaaaa"));
    assert!(reloaded.state.tasks.contains_key("tsq-bbbbbbbb"));
}

#[test]
fn invalid_latest_snapshot_is_skipped_and_full_replay_succeeds() {
    let dir = TempDir::new().expect("tempdir");
    let repo = dir.path();
    let task_id = "tsq-aaaaaaaa";
    append_events(repo, &[created_event(task_id, "first")]).expect("append events");

    let mut state = load_projected_state(repo).expect("load").state;
    state.tasks.get_mut(task_id).expect("task").priority = 9;
    state.applied_events = 1;
    let metadata = read_event_log_metadata(repo, 1).expect("metadata");
    let snapshot = Snapshot {
        taken_at: "2026-05-08T00:00:00.000Z".to_string(),
        event_count: 1,
        projection_version: STATE_CACHE_SCHEMA_VERSION,
        event_log: Some(metadata),
        state,
    };
    let paths = get_paths(repo);
    std::fs::create_dir_all(&paths.snapshots_dir).expect("create snapshots dir");
    std::fs::write(
        paths.snapshots_dir.join("2026-05-08T00-00-00-000Z-1.json"),
        serde_json::to_string_pretty(&snapshot).expect("serialize snapshot"),
    )
    .expect("write invalid snapshot");

    let reloaded = load_projected_state(repo).expect("reload");

    assert_eq!(reloaded.state.tasks.get(task_id).expect("task").priority, 0);
    assert!(reloaded.warning.unwrap_or_default().contains("Ignored invalid snapshot files"));
}
```

- [ ] **Step 2: Run tests to verify failures**

Run: `cargo test app::state`

Expected before implementation: invalid cache/snapshot tests fail because shortcut state is accepted.

- [ ] **Step 3: Add state invariant validator**

Create `src/domain/state_invariants.rs`:

```rust
use crate::errors::TsqError;
use crate::types::State;
use std::collections::HashSet;

pub fn validate_projected_state(state: &State) -> Result<(), TsqError> {
    for (id, task) in &state.tasks {
        if id.is_empty() || task.id != *id {
            return Err(invalid_state(format!("task map key \"{}\" does not match task id \"{}\"", id, task.id)));
        }
        if task.priority > 3 {
            return Err(invalid_state(format!("task {} priority {} is outside 0..=3", id, task.priority)));
        }
        for (field, target) in [
            ("parent_id", task.parent_id.as_ref()),
            ("superseded_by", task.superseded_by.as_ref()),
            ("duplicate_of", task.duplicate_of.as_ref()),
            ("replies_to", task.replies_to.as_ref()),
            ("discovered_from", task.discovered_from.as_ref()),
        ] {
            if let Some(target) = target
                && !state.tasks.contains_key(target)
            {
                return Err(invalid_state(format!("task {} {} references missing task {}", id, field, target)));
            }
        }
    }

    for id in state.tasks.keys() {
        let mut seen = HashSet::new();
        let mut cursor = Some(id.as_str());
        while let Some(current) = cursor {
            if !seen.insert(current.to_string()) {
                return Err(invalid_state(format!("parent cycle includes task {}", current)));
            }
            cursor = state.tasks.get(current).and_then(|task| task.parent_id.as_deref());
        }
    }

    for (dependent, blockers) in &state.deps {
        if !state.tasks.contains_key(dependent) {
            return Err(invalid_state(format!("dependency source {} references missing task", dependent)));
        }
        for blocker in blockers {
            if !state.tasks.contains_key(&blocker.blocker) {
                return Err(invalid_state(format!("dependency {} blocker {} references missing task", dependent, blocker.blocker)));
            }
        }
    }

    for (source, by_type) in &state.links {
        if !state.tasks.contains_key(source) {
            return Err(invalid_state(format!("link source {} references missing task", source)));
        }
        for targets in by_type.values() {
            for target in targets {
                if !state.tasks.contains_key(target) {
                    return Err(invalid_state(format!("link {} target {} references missing task", source, target)));
                }
            }
        }
    }

    let mut order_seen = HashSet::new();
    for id in &state.created_order {
        if !state.tasks.contains_key(id) {
            return Err(invalid_state(format!("created_order references missing task {}", id)));
        }
        if !order_seen.insert(id) {
            return Err(invalid_state(format!("created_order contains duplicate task {}", id)));
        }
    }

    Ok(())
}

fn invalid_state(message: String) -> TsqError {
    TsqError::new("STATE_INVALID", message, 2)
}
```

In `src/domain/mod.rs`, add:

```rust
pub mod state_invariants;
```

- [ ] **Step 4: Use validator in cache reads**

In `src/store/state.rs`, import:

```rust
use crate::domain::state_invariants::validate_projected_state;
```

Change successful `StateCache` branch:

```rust
if cache.schema_version == STATE_CACHE_SCHEMA_VERSION {
    if validate_projected_state(&cache.state).is_ok() {
        return Ok(Some(cache));
    }
    return Ok(None);
}
```

Change legacy `State` branch:

```rust
Ok(state) => {
    if validate_projected_state(&state).is_ok() {
        return Ok(Some(StateCache {
            schema_version: SCHEMA_VERSION,
            event_log: None,
            state,
        }));
    }
    return Ok(None);
}
```

Also change primary-cache handling so legacy `.tasque/tasks.jsonl` is considered only when `.tasque/state.json` is missing. If primary exists but has wrong schema, malformed JSON, or invalid invariant state, return `Ok(None)` and let callers replay events. This prevents stale legacy cache from shadowing a bad primary cache.

Use this shape:

```rust
let primary = paths.state_file;
let legacy = paths.tasque_dir.join("tasks.jsonl");

match read_to_string(&primary) {
    Ok(raw) => return parse_state_cache_candidate(&raw, true),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(error) => {
        return Err(
            TsqError::new("STATE_READ_FAILED", "Failed reading state cache", 2)
                .with_details(io_error_value(&error)),
        );
    }
}

match read_to_string(&legacy) {
    Ok(raw) => parse_state_cache_candidate(&raw, false),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(error) => Err(
        TsqError::new("STATE_READ_FAILED", "Failed reading state cache", 2)
            .with_details(io_error_value(&error)),
    ),
}
```

`parse_state_cache_candidate(raw, primary)` should return `Ok(None)` for invalid primary and for invalid legacy; only valid `StateCache` with current schema or valid legacy `State` can return `Ok(Some(...))`.

- [ ] **Step 5: Use validator in snapshot acceptance**

In `src/store/snapshots.rs`, import:

```rust
use crate::domain::state_invariants::validate_projected_state;
```

Change `is_snapshot`:

```rust
fn is_snapshot(snapshot: &Snapshot) -> bool {
    !snapshot.taken_at.is_empty()
        && snapshot.projection_version == STATE_CACHE_SCHEMA_VERSION
        && snapshot.event_log.as_ref().is_some_and(|event_log| {
            event_log.event_count == snapshot.event_count
                && event_log.event_count == snapshot.state.applied_events
        })
        && validate_projected_state(&snapshot.state).is_ok()
}
```

Update `src/app/state.rs` so invalid-snapshot warnings survive full replay fallback. Add a small result wrapper:

```rust
struct SnapshotLoadResult {
    loaded: Option<LoadedState>,
    warning: Option<String>,
}
```

Change `load_from_snapshot` to return `Result<SnapshotLoadResult, TsqError>`. When `load_latest_snapshot_with_warning` returns no snapshot, return:

```rust
return Ok(SnapshotLoadResult {
    loaded: None,
    warning: loaded.warning,
});
```

When snapshot metadata checks fail, preserve `loaded.warning` the same way. When snapshot succeeds, return `loaded: Some(LoadedState { warning: combine_warnings(tail.warning, loaded.warning), ... })`.

Change `load_projected_state_inner`:

```rust
let mut shortcut_warning = None;
if !include_events {
    if let Some(loaded) = load_from_state_cache(repo_root)? {
        return Ok(loaded);
    }
    let snapshot_result = load_from_snapshot(repo_root)?;
    if let Some(loaded) = snapshot_result.loaded {
        return Ok(loaded);
    }
    shortcut_warning = snapshot_result.warning;
}
```

Then full replay returns:

```rust
warning: combine_warnings(event_warning, shortcut_warning),
```

- [ ] **Step 6: Verify state task**

Run: `cargo test app::state`

Expected: cache and snapshot fallback tests pass.

### Task 3: Lock Behavior Tests

**Files:**
- Modify: `src/store/lock.rs`

- [ ] **Step 1: Add tests for lock safety**

At bottom of `src/store/lock.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::paths::get_paths;
    use tempfile::TempDir;

    fn write_lock(path: &Path, payload: &LockPayload) {
        std::fs::write(path, format!("{}\n", serde_json::to_string(payload).expect("serialize lock")))
            .expect("write lock");
    }

    #[test]
    fn live_lock_times_out() {
        let dir = TempDir::new().expect("tempdir");
        let paths = get_paths(dir.path());
        std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");
        let payload = LockPayload {
            host: System::host_name().unwrap_or_else(|| "unknown".to_string()),
            pid: std::process::id(),
            created_at: Utc::now().to_rfc3339(),
        };
        write_lock(&paths.lock_file, &payload);

        let err = acquire_write_lock_with_timeout(&paths.lock_file, &paths.tasque_dir, 1)
            .expect_err("live lock should time out");

        assert_eq!(err.code, "LOCK_TIMEOUT");
    }

    #[test]
    fn same_host_dead_pid_stale_lock_is_removed() {
        let dir = TempDir::new().expect("tempdir");
        let paths = get_paths(dir.path());
        std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");
        let payload = LockPayload {
            host: System::host_name().unwrap_or_else(|| "unknown".to_string()),
            pid: u32::MAX,
            created_at: (Utc::now() - chrono::Duration::milliseconds(STALE_LOCK_MS + 1_000)).to_rfc3339(),
        };
        write_lock(&paths.lock_file, &payload);

        let owned = acquire_write_lock_with_timeout(&paths.lock_file, &paths.tasque_dir, 200)
            .expect("stale lock should be replaced");

        assert_eq!(owned.pid, std::process::id());
        release_write_lock(&paths.lock_file, &owned).expect("release owned lock");
    }

    #[test]
    fn release_wrong_owner_reports_error_and_keeps_lock() {
        let dir = TempDir::new().expect("tempdir");
        let paths = get_paths(dir.path());
        std::fs::create_dir_all(&paths.tasque_dir).expect("create tasque dir");
        let owned = LockPayload {
            host: "host-a".to_string(),
            pid: 1,
            created_at: "2026-05-08T00:00:00Z".to_string(),
        };
        let other = LockPayload {
            host: "host-b".to_string(),
            pid: 2,
            created_at: "2026-05-08T00:00:01Z".to_string(),
        };
        write_lock(&paths.lock_file, &other);

        let err = release_write_lock(&paths.lock_file, &owned).expect_err("wrong owner should fail");

        assert_eq!(err.code, "LOCK_OWNERSHIP_MISMATCH");
        let raw = std::fs::read_to_string(&paths.lock_file).expect("lock still exists");
        assert!(raw.contains("host-b"));
    }

    #[test]
    fn with_write_lock_releases_lock_when_callback_errors() {
        let dir = TempDir::new().expect("tempdir");
        let repo = dir.path();
        let paths = get_paths(repo);

        let err = with_write_lock(repo, || {
            Err::<(), TsqError>(TsqError::new("CALLBACK_FAILED", "callback failed", 2))
        })
        .expect_err("callback should fail");

        assert_eq!(err.code, "CALLBACK_FAILED");
        assert!(!paths.lock_file.exists());
    }
}
```

- [ ] **Step 2: Run tests to verify failures**

Run: `cargo test store::lock`

Expected before implementation: tests fail because `acquire_write_lock_with_timeout` does not exist and release mismatch does not error.

- [ ] **Step 3: Add timeout helper and ownership mismatch error**

Refactor `acquire_write_lock`:

```rust
fn acquire_write_lock(lock_file: &Path, tasque_dir: &Path) -> Result<LockPayload, TsqError> {
    acquire_write_lock_with_timeout(lock_file, tasque_dir, lock_timeout_ms())
}

fn acquire_write_lock_with_timeout(
    lock_file: &Path,
    tasque_dir: &Path,
    timeout_ms: u64,
) -> Result<LockPayload, TsqError> {
    let deadline = SystemTime::now()
        .checked_add(Duration::from_millis(timeout_ms))
        .unwrap_or(SystemTime::now());
    let host = System::host_name().unwrap_or_else(|| "unknown".to_string());
    // Move existing acquire loop body here and use `timeout_ms` in LOCK_TIMEOUT details.
}
```

Change ownership mismatch branch in `release_write_lock`:

```rust
if payload.host != owned.host
    || payload.pid != owned.pid
    || payload.created_at != owned.created_at
{
    return Err(TsqError::new(
        "LOCK_OWNERSHIP_MISMATCH",
        "Lock file is owned by another writer",
        2,
    )
    .with_details(serde_json::json!({
        "lockFile": lock_file.display().to_string(),
        "owner": payload,
        "attempted_owner": owned,
    })));
}
```

- [ ] **Step 4: Verify lock task**

Run: `cargo test store::lock`

Expected: all lock tests pass.

### Task 4: `find --full` Documentation Drift

**Files:**
- Modify: `README.md`
- Modify: `npm/README.md`
- Modify: `AGENTS-reference.md`
- Modify: `SKILLS/tasque/references/command-reference.md`

- [ ] **Step 1: Patch command reference wording**

In each file, keep command list but add this note near the `find` commands:

```markdown
Note: for `find ready` and status-based `find` commands, `--full` is only valid with `--tree`. `find search --full` remains valid without `--tree`.
```

Where command synopsis is easy to change without making examples noisy, prefer:

```markdown
- `tsq find ready [filters...] [--tree [--full]]`
- `tsq find <blocked|open|in-progress|deferred|done|canceled> [filters...] [--tree [--full]]`
- `tsq find search <query> [--full]`
```

- [ ] **Step 2: Verify docs**

Run:

```bash
rg -n -e "--full" -e "find search" README.md npm/README.md AGENTS-reference.md SKILLS/tasque/references/command-reference.md
```

Expected: each doc includes the note and still lists `tsq find search <query> [--full]`.

### Task 5: Integration Verification

**Files:**
- No planned source changes.

- [ ] **Step 1: Format code**

Run: `cargo fmt`

Expected: no errors.

- [ ] **Step 2: Run targeted tests**

Run:

```bash
cargo test --test event_read_validation
cargo test app::state
cargo test store::lock
```

Expected: all targeted tests pass.

- [ ] **Step 3: Run full gates**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --quiet
```

Expected: all gates pass.

- [ ] **Step 4: Review diff**

Run:

```bash
git diff --check
git status --short
git diff -- README.md npm/README.md AGENTS-reference.md SKILLS/tasque/references/command-reference.md
```

Expected: no whitespace errors; diff limited to spec/plan plus durability/correctness files.

## Self-Review

- Spec coverage: corrupt-tail append, write validation, cache validation, snapshot validation, lock tests, docs drift, and full gates all have tasks.
- Placeholder scan: no `TBD`, `TODO`, or “add tests” without concrete tests.
- Type consistency: plan uses current `EventRecord`, `EventType`, `StateCache`, `Snapshot`, `STATE_CACHE_SCHEMA_VERSION`, `TsqError`, and existing module paths.
