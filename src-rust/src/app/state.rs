use crate::domain::projector::apply_events;
use crate::domain::state::create_empty_state;
use crate::errors::TsqError;
use crate::store::config::read_config;
use crate::store::events::read_events;
use crate::store::snapshots::{load_latest_snapshot_with_warning, write_snapshot};
use crate::store::state::{read_state_cache, write_state_cache};
use crate::types::{EventRecord, Snapshot, State};
use chrono::{SecondsFormat, Utc};
use std::path::Path;

pub struct LoadedState {
    pub state: State,
    pub all_events: Vec<EventRecord>,
    pub warning: Option<String>,
    pub snapshot: Option<Snapshot>,
}

pub fn load_projected_state(repo_root: impl AsRef<Path>) -> Result<LoadedState, TsqError> {
    let read = read_events(&repo_root)?;
    let events = read.events;
    let event_warning = read.warning;

    if let Some(from_cache) = read_state_cache(&repo_root)?
        && from_cache.applied_events <= events.len()
    {
        let offset = from_cache.applied_events;
        if offset == events.len() {
            return Ok(LoadedState {
                state: from_cache,
                all_events: events,
                warning: event_warning,
                snapshot: None,
            });
        }
        let mut state = apply_events(&from_cache, &events[offset..])?;
        state.applied_events = events.len();
        return Ok(LoadedState {
            state,
            all_events: events,
            warning: event_warning,
            snapshot: None,
        });
    }

    let loaded = load_latest_snapshot_with_warning(&repo_root)?;
    let base = loaded
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.state.clone())
        .unwrap_or_else(create_empty_state);
    let start_offset = loaded
        .snapshot
        .as_ref()
        .map(|snapshot| std::cmp::min(snapshot.event_count, events.len()))
        .unwrap_or(0);
    let mut projected = apply_events(&base, &events[start_offset..])?;
    projected.applied_events = events.len();

    Ok(LoadedState {
        state: projected,
        all_events: events,
        warning: combine_warnings(event_warning, loaded.warning),
        snapshot: loaded.snapshot,
    })
}

pub fn persist_projection(
    repo_root: impl AsRef<Path>,
    state: &mut State,
    event_count: usize,
    now: Option<&dyn Fn() -> String>,
) -> Result<(), TsqError> {
    state.applied_events = event_count;
    write_state_cache(&repo_root, state)?;

    let config = read_config(&repo_root)?;
    if config.snapshot_every == 0 {
        return Ok(());
    }

    if event_count > 0 && event_count.is_multiple_of(config.snapshot_every) {
        let taken_at = match now {
            Some(clock) => clock(),
            None => Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        };
        let snapshot = Snapshot {
            taken_at,
            event_count,
            state: state.clone(),
        };
        write_snapshot(&repo_root, &snapshot)?;
    }

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
