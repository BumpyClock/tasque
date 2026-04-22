use crate::app::sync;
use crate::domain::projector::apply_events;
use crate::domain::state::create_empty_state;
use crate::errors::TsqError;
use crate::store::config::read_config;
use crate::store::events::{read_event_log_metadata, read_events, read_events_tail_from_path};
use crate::store::paths::get_paths;
use crate::store::snapshots::{load_latest_snapshot_with_warning, write_snapshot};
use crate::store::state::{read_state_cache, write_state_cache};
use crate::types::{EventRecord, STATE_CACHE_SCHEMA_VERSION, Snapshot, State};
use chrono::{SecondsFormat, Utc};
use std::path::Path;

pub struct LoadedState {
    pub state: State,
    pub all_events: Vec<EventRecord>,
    pub event_count: usize,
    pub warning: Option<String>,
    pub snapshot: Option<Snapshot>,
}

pub fn load_projected_state(repo_root: impl AsRef<Path>) -> Result<LoadedState, TsqError> {
    load_projected_state_inner(repo_root, false)
}

pub fn load_projected_state_with_events(
    repo_root: impl AsRef<Path>,
) -> Result<LoadedState, TsqError> {
    load_projected_state_inner(repo_root, true)
}

fn load_projected_state_inner(
    repo_root: impl AsRef<Path>,
    include_events: bool,
) -> Result<LoadedState, TsqError> {
    let repo_root = repo_root.as_ref();

    if !include_events {
        if let Some(loaded) = load_from_state_cache(repo_root)? {
            return Ok(loaded);
        }
        if let Some(loaded) = load_from_snapshot(repo_root)? {
            return Ok(loaded);
        }
    }

    let read = read_events(repo_root)?;
    let event_count = read.metadata.event_count;
    let events = read.events;
    let event_warning = read.warning;

    let mut projected = apply_events(&create_empty_state(), &events)?;
    projected.applied_events = events.len();

    Ok(LoadedState {
        state: projected,
        all_events: if include_events { events } else { Vec::new() },
        event_count,
        warning: event_warning,
        snapshot: None,
    })
}

fn load_from_state_cache(repo_root: &Path) -> Result<Option<LoadedState>, TsqError> {
    let Some(cache) = read_state_cache(repo_root)? else {
        return Ok(None);
    };
    let Some(metadata) = cache.event_log.as_ref() else {
        return Ok(None);
    };
    if cache.state.applied_events != metadata.event_count {
        return Ok(None);
    }

    let events_file = get_paths(repo_root).events_file;
    let Some(tail) = read_events_tail_from_path(&events_file, metadata)? else {
        return Ok(None);
    };
    let mut state = if tail.events.is_empty() {
        cache.state
    } else {
        apply_events(&cache.state, &tail.events)?
    };
    state.applied_events = tail.metadata.event_count;

    Ok(Some(LoadedState {
        state,
        all_events: Vec::new(),
        event_count: tail.metadata.event_count,
        warning: tail.warning,
        snapshot: None,
    }))
}

fn load_from_snapshot(repo_root: &Path) -> Result<Option<LoadedState>, TsqError> {
    let loaded = load_latest_snapshot_with_warning(repo_root)?;
    let Some(snapshot) = loaded.snapshot else {
        return Ok(None);
    };
    let Some(metadata) = snapshot.event_log.as_ref() else {
        return Ok(None);
    };
    if snapshot.event_count != metadata.event_count
        || snapshot.state.applied_events != metadata.event_count
    {
        return Ok(None);
    }

    let events_file = get_paths(repo_root).events_file;
    let Some(tail) = read_events_tail_from_path(&events_file, metadata)? else {
        return Ok(None);
    };
    let mut state = if tail.events.is_empty() {
        snapshot.state.clone()
    } else {
        apply_events(&snapshot.state, &tail.events)?
    };
    state.applied_events = tail.metadata.event_count;

    Ok(Some(LoadedState {
        state,
        all_events: Vec::new(),
        event_count: tail.metadata.event_count,
        warning: combine_warnings(tail.warning, loaded.warning),
        snapshot: Some(snapshot),
    }))
}

pub fn persist_projection(
    repo_root: impl AsRef<Path>,
    state: &mut State,
    event_count: usize,
    now: Option<&dyn Fn() -> String>,
) -> Result<(), TsqError> {
    let repo_path = repo_root.as_ref();
    state.applied_events = event_count;
    let event_log = read_event_log_metadata(repo_path, event_count)?;
    write_state_cache(repo_path, state, event_log.clone())?;

    let config = read_config(repo_path)?;
    if config.snapshot_every == 0 {
        sync::auto_commit_if_sync_worktree(repo_path)?;
        return Ok(());
    }

    if event_count > 0 && event_count % config.snapshot_every == 0 {
        let taken_at = match now {
            Some(clock) => clock(),
            None => Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        };
        let snapshot = Snapshot {
            taken_at,
            event_count,
            projection_version: STATE_CACHE_SCHEMA_VERSION,
            event_log: Some(event_log),
            state: state.clone(),
        };
        write_snapshot(repo_path, &snapshot)?;
    }

    sync::auto_commit_if_sync_worktree(repo_path)?;
    Ok(())
}

fn combine_warnings(warnings: Option<String>, other: Option<String>) -> Option<String> {
    let mut combined = Vec::new();
    if let Some(value) = warnings
        && !value.is_empty()
    {
        combined.push(value);
    }
    if let Some(value) = other
        && !value.is_empty()
    {
        combined.push(value);
    }
    if combined.is_empty() {
        None
    } else {
        Some(combined.join(" | "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::events::make_event;
    use crate::store::events::{append_events, read_event_log_metadata};
    use crate::store::paths::get_paths;
    use crate::types::{EventType, SCHEMA_VERSION, StateCache, TaskStatus};
    use serde_json::Map;
    use tempfile::TempDir;

    fn created_event(task_id: &str, title: &str) -> EventRecord {
        let mut payload = Map::new();
        payload.insert("title".to_string(), serde_json::json!(title));
        make_event(
            "test",
            "2026-01-01T00:00:00.000Z",
            EventType::TaskCreated,
            task_id,
            payload,
        )
    }

    fn updated_event(task_id: &str, payload: serde_json::Value) -> EventRecord {
        make_event(
            "test",
            "2026-01-02T00:00:00.000Z",
            EventType::TaskUpdated,
            task_id,
            payload.as_object().cloned().unwrap_or_default(),
        )
    }

    #[test]
    fn stale_state_cache_is_ignored_after_same_count_log_rewrite() {
        let dir = TempDir::new().expect("tempdir");
        let repo = dir.path();
        let first = created_event("tsq-aaaaaaaa", "first");
        append_events(repo, &[first]).expect("append first event");

        let mut loaded = load_projected_state(repo).expect("load first state").state;
        persist_projection(repo, &mut loaded, 1, None).expect("persist cache");

        let paths = get_paths(repo);
        std::fs::write(
            &paths.events_file,
            format!(
                "{}\n",
                serde_json::to_string(&created_event("tsq-bbbbbbbb", "second"))
                    .expect("serialize event")
            ),
        )
        .expect("rewrite events");

        let reloaded = load_projected_state(repo).expect("reload rewritten state");
        assert!(reloaded.state.tasks.contains_key("tsq-bbbbbbbb"));
        assert!(!reloaded.state.tasks.contains_key("tsq-aaaaaaaa"));
    }

    #[test]
    fn state_cache_replays_only_tail_when_prefix_matches() {
        let dir = TempDir::new().expect("tempdir");
        let repo = dir.path();
        let first = created_event("tsq-aaaaaaaa", "first");
        append_events(repo, &[first]).expect("append first event");

        let mut loaded = load_projected_state(repo).expect("load first state").state;
        persist_projection(repo, &mut loaded, 1, None).expect("persist cache");

        let second = created_event("tsq-bbbbbbbb", "second");
        append_events(repo, &[second]).expect("append second event");

        let reloaded = load_projected_state(repo).expect("reload tail state");
        assert_eq!(reloaded.event_count, 2);
        assert!(reloaded.state.tasks.contains_key("tsq-aaaaaaaa"));
        assert!(reloaded.state.tasks.contains_key("tsq-bbbbbbbb"));
        assert!(reloaded.all_events.is_empty());
    }

    #[test]
    fn old_state_cache_schema_is_ignored_after_projection_semantics_change() {
        let dir = TempDir::new().expect("tempdir");
        let repo = dir.path();
        let task_id = "tsq-aaaaaaaa";
        let created = created_event(task_id, "first");
        let closed = updated_event(task_id, serde_json::json!({"status": "closed"}));
        append_events(repo, &[created.clone(), closed]).expect("append events");

        let mut stale_state =
            apply_events(&create_empty_state(), &[created]).expect("project created");
        stale_state.applied_events = 2;
        let metadata = read_event_log_metadata(repo, 2).expect("metadata");
        let stale_cache = StateCache {
            schema_version: SCHEMA_VERSION,
            event_log: Some(metadata),
            state: stale_state,
        };
        let paths = get_paths(repo);
        std::fs::write(
            &paths.state_file,
            serde_json::to_string_pretty(&stale_cache).expect("serialize cache"),
        )
        .expect("write stale cache");

        let reloaded = load_projected_state(repo).expect("reload state");
        let task = reloaded.state.tasks.get(task_id).expect("task");
        assert_eq!(task.status, TaskStatus::Closed);
    }
}
