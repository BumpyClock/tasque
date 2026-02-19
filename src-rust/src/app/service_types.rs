pub use crate::app::storage::{SpecCheckDiagnostic, SpecCheckResult};
use crate::domain::dep_tree::DepDirection;
use crate::domain::validate::PlanningLane;
use crate::skills::types::SkillOperationSummary;
pub use crate::skills::types::SkillTarget;
use crate::types::{
    DependencyType, EventRecord, PlanningState, Priority, RelationType, RepairDep, Task, TaskKind,
    TaskNote, TaskStatus,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitResult {
    pub initialized: bool,
    pub files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_operation: Option<SkillOperationSummary>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InitInput {
    pub install_skill: bool,
    pub uninstall_skill: bool,
    pub skill_targets: Option<Vec<SkillTarget>>,
    pub skill_name: Option<String>,
    pub force_skill_overwrite: bool,
    pub skill_dir_claude: Option<String>,
    pub skill_dir_codex: Option<String>,
    pub skill_dir_copilot: Option<String>,
    pub skill_dir_opencode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInput {
    pub title: String,
    pub kind: TaskKind,
    pub priority: Priority,
    pub description: Option<String>,
    pub external_ref: Option<String>,
    pub discovered_from: Option<String>,
    pub parent: Option<String>,
    pub exact_id: bool,
    pub planning_state: Option<PlanningState>,
    pub explicit_id: Option<String>,
    pub body_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInput {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub clear_description: bool,
    pub external_ref: Option<String>,
    pub discovered_from: Option<String>,
    pub clear_discovered_from: bool,
    pub clear_external_ref: bool,
    pub status: Option<TaskStatus>,
    pub priority: Option<Priority>,
    pub exact_id: bool,
    pub planning_state: Option<PlanningState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimInput {
    pub id: String,
    pub assignee: Option<String>,
    pub require_spec: bool,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkInput {
    pub src: String,
    pub dst: String,
    #[serde(rename = "type")]
    pub rel_type: RelationType,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepInput {
    pub child: String,
    pub blocker: String,
    pub dep_type: Option<DependencyType>,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupersedeInput {
    pub source: String,
    pub with_id: String,
    pub reason: Option<String>,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateInput {
    pub source: String,
    pub canonical: String,
    pub reason: Option<String>,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseInput {
    pub ids: Vec<String>,
    pub reason: Option<String>,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReopenInput {
    pub ids: Vec<String>,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryInput {
    pub id: String,
    pub limit: Option<usize>,
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    pub actor: Option<String>,
    pub since: Option<String>,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryResult {
    pub events: Vec<EventRecord>,
    pub count: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateCandidateGroup {
    pub key: String,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateCandidatesResult {
    pub scanned: usize,
    pub groups: Vec<DuplicateCandidateGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelInput {
    pub id: String,
    pub label: String,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelCount {
    pub label: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepTreeInput {
    pub id: String,
    pub direction: Option<DepDirection>,
    pub depth: Option<usize>,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteAddInput {
    pub id: String,
    pub text: String,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteListInput {
    pub id: String,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecAttachInput {
    pub id: String,
    pub file: Option<String>,
    pub source: Option<String>,
    pub text: Option<String>,
    pub stdin: bool,
    pub force: bool,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecCheckInput {
    pub id: String,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteAddResult {
    pub task_id: String,
    pub note: TaskNote,
    pub notes_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteListResult {
    pub task_id: String,
    pub notes: Vec<TaskNote>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecAttachResult {
    pub task: Task,
    pub spec: SpecAttachSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecAttachSpec {
    pub spec_path: String,
    pub spec_fingerprint: String,
    pub spec_attached_at: String,
    pub spec_attached_by: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchInput {
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListFilter {
    pub statuses: Option<Vec<TaskStatus>>,
    pub assignee: Option<String>,
    pub external_ref: Option<String>,
    pub discovered_from: Option<String>,
    pub kind: Option<TaskKind>,
    pub label: Option<String>,
    pub label_any: Option<Vec<String>>,
    pub created_after: Option<String>,
    pub updated_after: Option<String>,
    pub closed_after: Option<String>,
    pub unassigned: bool,
    pub ids: Option<Vec<String>>,
    pub planning_state: Option<PlanningState>,
    pub dep_type: Option<DependencyType>,
    pub dep_direction: Option<DepDirectionFilter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepDirectionFilter {
    In,
    Out,
    Any,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadyInput {
    pub lane: Option<PlanningLane>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeInput {
    pub sources: Vec<String>,
    pub into: String,
    pub reason: Option<String>,
    pub force: bool,
    pub dry_run: bool,
    pub exact_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeSummary {
    pub requested_sources: usize,
    pub merged_sources: usize,
    pub skipped_sources: usize,
    pub planned_events: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeProjected {
    pub target: Task,
    pub sources: Vec<Task>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeItem {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeTarget {
    pub id: String,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub merged: Vec<MergeItem>,
    pub target: MergeTarget,
    pub dry_run: bool,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_summary: Option<MergeSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projected: Option<MergeProjected>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleInput {
    pub days: i64,
    pub status: Option<TaskStatus>,
    pub assignee: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleResult {
    pub tasks: Vec<Task>,
    pub days: i64,
    pub cutoff: String,
    pub statuses: Vec<TaskStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorResult {
    pub tasks: usize,
    pub events: usize,
    pub snapshot_loaded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrphanedLinkResult {
    pub src: String,
    pub dst: String,
    #[serde(rename = "type")]
    pub rel_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrphansResult {
    pub orphaned_deps: Vec<RepairDep>,
    pub orphaned_links: Vec<OrphanedLinkResult>,
    pub total: usize,
}

#[derive(Clone)]
pub struct ServiceContext {
    pub repo_root: String,
    pub actor: String,
    pub now: Arc<dyn Fn() -> String + Send + Sync>,
}
