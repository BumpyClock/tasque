use serde::{Deserialize, Serialize};
use serde_json::Map;
use serde_json::Value;
use std::collections::HashMap;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Task,
    Feature,
    Epic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    InProgress,
    Blocked,
    Closed,
    Canceled,
    Deferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanningState {
    NeedsPlanning,
    Planned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyType {
    Blocks,
    StartsAfter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    RelatesTo,
    RepliesTo,
    Duplicates,
    Supersedes,
}

pub type Priority = u8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskNote {
    pub event_id: String,
    pub ts: String,
    pub actor: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub kind: TaskKind,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub notes: Vec<TaskNote>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec_attached_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec_attached_by: Option<String>,
    pub status: TaskStatus,
    pub priority: Priority,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovered_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_of: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planning_state: Option<PlanningState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replies_to: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DependencyEdge {
    pub blocker: String,
    pub dep_type: DependencyType,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DependencyEdgeWire {
    String(String),
    Object {
        blocker: String,
        dep_type: Option<DependencyType>,
    },
}

impl<'de> Deserialize<'de> for DependencyEdge {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = DependencyEdgeWire::deserialize(deserializer)?;
        match wire {
            DependencyEdgeWire::String(blocker) => Ok(DependencyEdge {
                blocker,
                dep_type: DependencyType::Blocks,
            }),
            DependencyEdgeWire::Object { blocker, dep_type } => Ok(DependencyEdge {
                blocker,
                dep_type: dep_type.unwrap_or(DependencyType::Blocks),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyRef {
    pub id: String,
    pub dep_type: DependencyType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskTreeNode {
    pub task: Task,
    pub blockers: Vec<String>,
    pub dependents: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocker_edges: Option<Vec<DependencyRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependent_edges: Option<Vec<DependencyRef>>,
    pub children: Vec<TaskTreeNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    #[serde(rename = "task.created")]
    TaskCreated,
    #[serde(rename = "task.updated")]
    TaskUpdated,
    #[serde(rename = "task.status_set")]
    TaskStatusSet,
    #[serde(rename = "task.claimed")]
    TaskClaimed,
    #[serde(rename = "task.noted")]
    TaskNoted,
    #[serde(rename = "task.spec_attached")]
    TaskSpecAttached,
    #[serde(rename = "task.superseded")]
    TaskSuperseded,
    #[serde(rename = "dep.added")]
    DepAdded,
    #[serde(rename = "dep.removed")]
    DepRemoved,
    #[serde(rename = "link.added")]
    LinkAdded,
    #[serde(rename = "link.removed")]
    LinkRemoved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRecord {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    pub ts: String,
    pub actor: String,
    #[serde(rename = "type")]
    pub event_type: EventType,
    pub task_id: String,
    pub payload: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub tasks: HashMap<String, Task>,
    pub deps: HashMap<String, Vec<DependencyEdge>>,
    pub links: HashMap<String, HashMap<RelationType, Vec<String>>>,
    pub child_counters: HashMap<String, u32>,
    pub created_order: Vec<String>,
    pub applied_events: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub taken_at: String,
    pub event_count: usize,
    pub state: State,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub schema_version: u32,
    pub snapshot_every: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeOk<T> {
    pub schema_version: u32,
    pub command: String,
    pub ok: bool,
    pub data: T,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeErr {
    pub schema_version: u32,
    pub command: String,
    pub ok: bool,
    pub error: EnvelopeError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Envelope<T> {
    Ok(EnvelopeOk<T>),
    Err(EnvelopeErr),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepairPlan {
    pub orphaned_deps: Vec<RepairDep>,
    pub orphaned_links: Vec<RepairLink>,
    pub stale_temps: Vec<String>,
    pub stale_lock: bool,
    pub old_snapshots: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepairDep {
    pub child: String,
    pub blocker: String,
    pub dep_type: DependencyType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepairLink {
    pub src: String,
    pub dst: String,
    #[serde(rename = "type")]
    pub rel_type: RelationType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepairResult {
    pub plan: RepairPlan,
    pub applied: bool,
    pub events_appended: usize,
    pub files_removed: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeOptions {
    pub repo_root: String,
    pub actor: String,
}
