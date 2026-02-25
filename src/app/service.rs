#[path = "service_create_update.rs"]
mod service_create_update;
#[path = "service_labels.rs"]
mod service_labels;
#[path = "service_notes.rs"]
mod service_notes;
#[path = "service_specs.rs"]
mod service_specs;

use crate::app::repair::{RepairOptions, execute_repair};
use crate::app::service_types::*;
use crate::app::service_utils::must_resolve_existing;
use crate::app::storage::{
    ensure_events_file, ensure_tasque_gitignore, load_projected_state, write_default_config,
};
use crate::domain::dep_tree::build_dep_tree;
use crate::skills::{apply_skill_operation, types::SkillAction};
use crate::types::{DependencyType, RelationType, RepairResult, Task, TaskTreeNode};
use crate::{app::service_lifecycle, app::service_query, errors::TsqError};
use std::fs;
use std::sync::Arc;

pub use crate::app::service_query::ShowResult;

pub struct TasqueService {
    ctx: ServiceContext,
}

impl TasqueService {
    pub fn new(
        repo_root: impl Into<String>,
        actor: impl Into<String>,
        now: impl Fn() -> String + Send + Sync + 'static,
    ) -> Self {
        Self {
            ctx: ServiceContext {
                repo_root: repo_root.into(),
                actor: actor.into(),
                now: Arc::new(now),
            },
        }
    }

    pub fn init(&self, input: InitInput) -> Result<InitResult, TsqError> {
        if input.install_skill && input.uninstall_skill {
            return Err(TsqError::new(
                "VALIDATION_ERROR",
                "cannot combine --install-skill with --uninstall-skill",
                1,
            ));
        }

        write_default_config(&self.ctx.repo_root)?;
        ensure_events_file(&self.ctx.repo_root)?;
        fs::create_dir_all(format!("{}/.tasque/snapshots", self.ctx.repo_root)).map_err(|e| {
            TsqError::new("IO_ERROR", "failed to initialize .tasque/snapshots", 2)
                .with_details(io_error_value(&e))
        })?;
        ensure_tasque_gitignore(&self.ctx.repo_root)?;

        let files = vec![
            ".tasque/config.json".to_string(),
            ".tasque/events.jsonl".to_string(),
            ".tasque/.gitignore".to_string(),
        ];

        let sync_setup = if let Some(ref branch) = input.sync_branch {
            Some(crate::app::sync::setup_sync_branch(
                &self.ctx.repo_root,
                branch,
                &self.ctx.actor,
            )?)
        } else {
            None
        };

        let action = if input.install_skill {
            Some(SkillAction::Install)
        } else if input.uninstall_skill {
            Some(SkillAction::Uninstall)
        } else {
            None
        };

        if let Some(action) = action {
            let skill_operation =
                apply_skill_operation(crate::skills::types::SkillOperationOptions {
                    action,
                    skill_name: input
                        .skill_name
                        .clone()
                        .unwrap_or_else(|| "tasque".to_string()),
                    targets: input.skill_targets.clone().unwrap_or_else(|| {
                        vec![
                            SkillTarget::Claude,
                            SkillTarget::Codex,
                            SkillTarget::Copilot,
                            SkillTarget::Opencode,
                        ]
                    }),
                    force: input.force_skill_overwrite,
                    source_root_dir: None,
                    home_dir: None,
                    codex_home: None,
                    target_dir_overrides: build_target_overrides(&input),
                })?;
            return Ok(InitResult {
                initialized: true,
                files,
                skill_operation: Some(skill_operation),
                sync_setup,
            });
        }

        Ok(InitResult {
            initialized: true,
            files,
            skill_operation: None,
            sync_setup,
        })
    }

    pub fn create(&self, input: CreateInput) -> Result<Task, TsqError> {
        service_create_update::create(&self.ctx, &input)
    }

    pub fn show(&self, id_raw: &str, exact_id: bool) -> Result<ShowResult, TsqError> {
        service_query::show(&self.ctx, id_raw, exact_id)
    }

    pub fn list(&self, filter: &ListFilter) -> Result<Vec<Task>, TsqError> {
        service_query::list(&self.ctx, filter)
    }

    pub fn stale(&self, input: &StaleInput) -> Result<StaleResult, TsqError> {
        service_query::stale(&self.ctx, input)
    }

    pub fn list_tree(&self, filter: &ListFilter) -> Result<Vec<TaskTreeNode>, TsqError> {
        service_query::list_tree(&self.ctx, filter)
    }

    pub fn ready(
        &self,
        lane: Option<crate::domain::validate::PlanningLane>,
    ) -> Result<Vec<Task>, TsqError> {
        service_query::ready(&self.ctx, lane)
    }

    pub fn doctor(&self) -> Result<DoctorResult, TsqError> {
        service_query::doctor(&self.ctx)
    }

    pub fn orphans(&self) -> Result<OrphansResult, TsqError> {
        service_query::orphans(&self.ctx)
    }

    pub fn update(&self, input: UpdateInput) -> Result<Task, TsqError> {
        service_create_update::update(&self.ctx, &input)
    }

    pub fn note_add(&self, input: NoteAddInput) -> Result<NoteAddResult, TsqError> {
        service_notes::note_add(&self.ctx, &input)
    }

    pub fn note_list(&self, input: NoteListInput) -> Result<NoteListResult, TsqError> {
        service_notes::note_list(&self.ctx, &input)
    }

    pub fn spec_attach(&self, input: SpecAttachInput) -> Result<SpecAttachResult, TsqError> {
        service_specs::spec_attach(&self.ctx, &input)
    }

    pub fn spec_check(&self, input: SpecCheckInput) -> Result<SpecCheckResult, TsqError> {
        service_specs::spec_check(&self.ctx, &input)
    }

    pub fn claim(&self, input: ClaimInput) -> Result<Task, TsqError> {
        service_lifecycle::claim(&self.ctx, &input)
    }

    pub fn dep_add(&self, input: DepInput) -> Result<(String, String, DependencyType), TsqError> {
        service_lifecycle::dep_add(&self.ctx, &input)
    }

    pub fn dep_remove(
        &self,
        input: DepInput,
    ) -> Result<(String, String, DependencyType), TsqError> {
        service_lifecycle::dep_remove(&self.ctx, &input)
    }

    pub fn link_add(&self, input: LinkInput) -> Result<(String, String, RelationType), TsqError> {
        service_lifecycle::link_add(&self.ctx, &input)
    }

    pub fn link_remove(
        &self,
        input: LinkInput,
    ) -> Result<(String, String, RelationType), TsqError> {
        service_lifecycle::link_remove(&self.ctx, &input)
    }

    pub fn supersede(&self, input: SupersedeInput) -> Result<Task, TsqError> {
        service_lifecycle::supersede(&self.ctx, &input)
    }

    pub fn duplicate(&self, input: DuplicateInput) -> Result<Task, TsqError> {
        service_lifecycle::duplicate(&self.ctx, &input)
    }

    pub fn merge(&self, input: MergeInput) -> Result<MergeResult, TsqError> {
        service_lifecycle::merge(&self.ctx, &input)
    }

    pub fn duplicate_candidates(
        &self,
        limit: Option<usize>,
    ) -> Result<DuplicateCandidatesResult, TsqError> {
        service_lifecycle::duplicate_candidates(&self.ctx, limit.unwrap_or(20))
    }

    pub fn repair(&self, fix: bool, force_unlock: bool) -> Result<RepairResult, TsqError> {
        execute_repair(
            &self.ctx.repo_root,
            &self.ctx.actor,
            self.ctx.now.as_ref(),
            RepairOptions { fix, force_unlock },
        )
    }

    pub fn close(&self, input: CloseInput) -> Result<Vec<Task>, TsqError> {
        service_lifecycle::close(&self.ctx, &input)
    }

    pub fn reopen(&self, input: ReopenInput) -> Result<Vec<Task>, TsqError> {
        service_lifecycle::reopen(&self.ctx, &input)
    }

    pub fn history(&self, input: HistoryInput) -> Result<HistoryResult, TsqError> {
        service_query::history(&self.ctx, &input)
    }

    pub fn label_add(&self, input: LabelInput) -> Result<Task, TsqError> {
        service_labels::label_add(&self.ctx, &input)
    }

    pub fn label_remove(&self, input: LabelInput) -> Result<Task, TsqError> {
        service_labels::label_remove(&self.ctx, &input)
    }

    pub fn label_list(&self) -> Result<Vec<LabelCount>, TsqError> {
        service_labels::label_list(&self.ctx)
    }

    pub fn dep_tree(
        &self,
        input: DepTreeInput,
    ) -> Result<crate::domain::dep_tree::DepTreeNode, TsqError> {
        let loaded = load_projected_state(&self.ctx.repo_root)?;
        let id = must_resolve_existing(&loaded.state, &input.id, input.exact_id)?;
        build_dep_tree(
            &loaded.state,
            &id,
            input
                .direction
                .unwrap_or(crate::domain::dep_tree::DepDirection::Both),
            input.depth.unwrap_or(10),
        )
    }

    pub fn search(&self, input: &SearchInput) -> Result<Vec<Task>, TsqError> {
        service_query::search(&self.ctx, input)
    }

    pub fn migrate(&self, branch: &str) -> Result<crate::types::MigrateResult, TsqError> {
        crate::app::sync::migrate_to_sync_branch(
            &self.ctx.repo_root,
            branch,
            &self.ctx.actor,
        )
    }
}

fn build_target_overrides(
    input: &InitInput,
) -> Option<std::collections::HashMap<SkillTarget, String>> {
    let mut map = std::collections::HashMap::new();
    if let Some(path) = input.skill_dir_claude.as_ref() {
        map.insert(SkillTarget::Claude, path.clone());
    }
    if let Some(path) = input.skill_dir_codex.as_ref() {
        map.insert(SkillTarget::Codex, path.clone());
    }
    if let Some(path) = input.skill_dir_copilot.as_ref() {
        map.insert(SkillTarget::Copilot, path.clone());
    }
    if let Some(path) = input.skill_dir_opencode.as_ref() {
        map.insert(SkillTarget::Opencode, path.clone());
    }
    if map.is_empty() { None } else { Some(map) }
}

fn io_error_value(error: &std::io::Error) -> serde_json::Value {
    serde_json::json!({"kind": format!("{:?}", error.kind()), "message": error.to_string()})
}
