use crate::app::state::{load_projected_state, persist_projection};
use crate::domain::deps::normalize_dependency_edges;
use crate::domain::events::make_event;
use crate::domain::projector::apply_events;
use crate::errors::TsqError;
use crate::store::events::append_events;
use crate::store::lock::{force_remove_lock, lock_exists, with_write_lock};
use crate::store::paths::get_paths;
use crate::store::snapshots::SNAPSHOT_RETAIN_COUNT;
use crate::types::{EventRecord, RepairDep, RepairLink, RepairPlan, RepairResult, State};
use serde_json::{Map, Value};
use std::fs::read_dir;
use std::path::Path;

pub struct OrphanedGraph {
    pub orphaned_deps: Vec<RepairDep>,
    pub orphaned_links: Vec<RepairLink>,
}

pub struct RepairOptions {
    pub fix: bool,
    pub force_unlock: bool,
}

pub fn scan_orphaned_graph(state: &State) -> OrphanedGraph {
    let mut orphaned_deps = Vec::new();
    let mut orphaned_links = Vec::new();

    for (child, blockers) in &state.deps {
        for edge in normalize_dependency_edges(Some(blockers)) {
            if !state.tasks.contains_key(child) || !state.tasks.contains_key(&edge.blocker) {
                orphaned_deps.push(RepairDep {
                    child: child.to_string(),
                    blocker: edge.blocker,
                    dep_type: edge.dep_type,
                });
            }
        }
    }

    for (src, rels) in &state.links {
        for (kind, targets) in rels {
            for target in targets {
                if !state.tasks.contains_key(src) || !state.tasks.contains_key(target) {
                    orphaned_links.push(RepairLink {
                        src: src.to_string(),
                        dst: target.to_string(),
                        rel_type: *kind,
                    });
                }
            }
        }
    }

    OrphanedGraph {
        orphaned_deps,
        orphaned_links,
    }
}

fn scan_filesystem(
    repo_root: impl AsRef<Path>,
) -> Result<(Vec<String>, bool, Vec<String>), TsqError> {
    let paths = get_paths(&repo_root);
    let mut stale_temps = Vec::new();
    let mut old_snapshots = Vec::new();

    if let Ok(entries) = read_dir(&paths.tasque_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str()
                && name.contains(".tmp")
            {
                stale_temps.push(name.to_string());
            }
        }
    }

    let stale_lock = lock_exists(repo_root)?;

    if let Ok(entries) = read_dir(&paths.snapshots_dir) {
        let mut snapshots: Vec<String> = entries
            .flatten()
            .filter_map(|entry| entry.file_name().to_str().map(|name| name.to_string()))
            .filter(|name| name.ends_with(".json"))
            .collect();
        snapshots.sort();
        if snapshots.len() > SNAPSHOT_RETAIN_COUNT {
            old_snapshots = snapshots[..snapshots.len() - SNAPSHOT_RETAIN_COUNT].to_vec();
        }
    }

    Ok((stale_temps, stale_lock, old_snapshots))
}

pub fn execute_repair(
    repo_root: impl AsRef<Path>,
    actor: &str,
    now: &dyn Fn() -> String,
    opts: RepairOptions,
) -> Result<RepairResult, TsqError> {
    let repo_root = repo_root.as_ref().to_path_buf();

    if opts.force_unlock && !opts.fix {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "--force-unlock requires --fix",
            1,
        ));
    }

    if !opts.fix {
        let loaded = load_projected_state(&repo_root)?;
        let graph = scan_orphaned_graph(&loaded.state);
        let (stale_temps, stale_lock, old_snapshots) = scan_filesystem(&repo_root)?;
        let plan = RepairPlan {
            orphaned_deps: graph.orphaned_deps,
            orphaned_links: graph.orphaned_links,
            stale_temps,
            stale_lock,
            old_snapshots,
        };
        return Ok(RepairResult {
            plan,
            applied: false,
            events_appended: 0,
            files_removed: 0,
        });
    }

    if opts.force_unlock && lock_exists(&repo_root)? {
        let _ = force_remove_lock(&repo_root)?;
    }

    with_write_lock(&repo_root, || {
        let loaded = load_projected_state(&repo_root)?;
        let graph = scan_orphaned_graph(&loaded.state);
        let (stale_temps, stale_lock, old_snapshots) = scan_filesystem(&repo_root)?;
        let plan = RepairPlan {
            orphaned_deps: graph.orphaned_deps,
            orphaned_links: graph.orphaned_links,
            stale_temps,
            stale_lock,
            old_snapshots,
        };

        let mut events: Vec<EventRecord> = Vec::new();

        for dep in &plan.orphaned_deps {
            events.push(make_event(
                actor,
                &now(),
                crate::types::EventType::DepRemoved,
                &dep.child,
                payload_map(serde_json::json!({
                  "blocker": dep.blocker,
                  "dep_type": dep.dep_type,
                })),
            ));
        }

        for link in &plan.orphaned_links {
            events.push(make_event(
                actor,
                &now(),
                crate::types::EventType::LinkRemoved,
                &link.src,
                payload_map(serde_json::json!({
                  "type": link.rel_type,
                  "target": link.dst,
                })),
            ));
        }

        if !events.is_empty() {
            let mut next_state = apply_events(&loaded.state, &events)?;
            append_events(&repo_root, &events)?;
            persist_projection(
                &repo_root,
                &mut next_state,
                loaded.all_events.len() + events.len(),
                None,
            )?;
        }

        let mut files_removed = 0;
        let paths = get_paths(&repo_root);

        for temp in &plan.stale_temps {
            if std::fs::remove_file(paths.tasque_dir.join(temp)).is_ok() {
                files_removed += 1;
            }
        }

        for snap in &plan.old_snapshots {
            if std::fs::remove_file(paths.snapshots_dir.join(snap)).is_ok() {
                files_removed += 1;
            }
        }

        Ok(RepairResult {
            plan,
            applied: true,
            events_appended: events.len(),
            files_removed,
        })
    })
}

fn payload_map(value: Value) -> Map<String, Value> {
    match value.as_object() {
        Some(map) => map.clone(),
        None => Map::new(),
    }
}
