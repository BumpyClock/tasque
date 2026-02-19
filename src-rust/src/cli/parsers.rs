use crate::app::runtime::{normalize_status, parse_priority};
use crate::app::service_types::{DepDirectionFilter, ListFilter};
use crate::domain::dep_tree::DepDirection;
use crate::domain::validate::PlanningLane;
use crate::errors::TsqError;
use crate::skills::types::SkillTarget;
use crate::types::{DependencyType, PlanningState, RelationType, TaskKind, TaskStatus};
use once_cell::sync::Lazy;
use regex::Regex;

static EXPLICIT_ID_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^tsq-[0-9a-hjkmnp-tv-z]{8}$").expect("valid explicit id pattern"));
static ISO_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})$")
        .expect("valid iso timestamp pattern")
});

pub const TREE_DEFAULT_STATUSES: &[TaskStatus] = &[TaskStatus::Open, TaskStatus::InProgress];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitPreset {
    Minimal,
    Standard,
    Full,
}

#[derive(Debug, Clone)]
pub struct ListParseInput {
    pub status: Option<String>,
    pub assignee: Option<String>,
    pub unassigned: bool,
    pub has_assignee_flag: bool,
    pub external_ref: Option<String>,
    pub discovered_from: Option<String>,
    pub kind: Option<String>,
    pub label: Option<String>,
    pub label_any: Vec<String>,
    pub created_after: Option<String>,
    pub updated_after: Option<String>,
    pub closed_after: Option<String>,
    pub ids: Vec<String>,
    pub planning: Option<String>,
    pub dep_type: Option<String>,
    pub dep_direction: Option<String>,
}

pub fn as_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn parse_kind(raw: &str) -> Result<TaskKind, TsqError> {
    match raw {
        "task" => Ok(TaskKind::Task),
        "feature" => Ok(TaskKind::Feature),
        "epic" => Ok(TaskKind::Epic),
        _ => Err(TsqError::new(
            "VALIDATION_ERROR",
            "kind must be task|feature|epic",
            1,
        )),
    }
}

pub fn parse_relation_type(raw: &str) -> Result<RelationType, TsqError> {
    match raw {
        "relates_to" => Ok(RelationType::RelatesTo),
        "replies_to" => Ok(RelationType::RepliesTo),
        "duplicates" => Ok(RelationType::Duplicates),
        "supersedes" => Ok(RelationType::Supersedes),
        _ => Err(TsqError::new(
            "VALIDATION_ERROR",
            "relation type must be relates_to|replies_to|duplicates|supersedes",
            1,
        )),
    }
}

pub fn parse_planning_state(raw: &str) -> Result<PlanningState, TsqError> {
    match raw {
        "needs_planning" => Ok(PlanningState::NeedsPlanning),
        "planned" => Ok(PlanningState::Planned),
        _ => Err(TsqError::new(
            "VALIDATION_ERROR",
            "planning state must be needs_planning|planned",
            1,
        )),
    }
}

pub fn parse_init_preset(raw: &str) -> Result<InitPreset, TsqError> {
    match raw.trim().to_lowercase().as_str() {
        "minimal" => Ok(InitPreset::Minimal),
        "standard" => Ok(InitPreset::Standard),
        "full" => Ok(InitPreset::Full),
        _ => Err(TsqError::new(
            "VALIDATION_ERROR",
            "preset must be minimal|standard|full",
            1,
        )),
    }
}

pub fn validate_explicit_id(raw: &str) -> Result<String, TsqError> {
    let trimmed = raw.trim();
    if !EXPLICIT_ID_PATTERN.is_match(trimmed) {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "explicit --id must match tsq-<8 crockford base32 chars>",
            1,
        ));
    }
    Ok(trimmed.to_string())
}

pub fn parse_lane(raw: &str) -> Result<PlanningLane, TsqError> {
    match raw {
        "planning" => Ok(PlanningLane::Planning),
        "coding" => Ok(PlanningLane::Coding),
        _ => Err(TsqError::new(
            "VALIDATION_ERROR",
            "lane must be planning|coding",
            1,
        )),
    }
}

pub fn parse_skill_targets(raw: &str) -> Result<Vec<SkillTarget>, TsqError> {
    let tokens: Vec<String> = raw
        .split(',')
        .map(|entry| entry.trim().to_lowercase())
        .filter(|entry| !entry.is_empty())
        .collect();
    if tokens.is_empty() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "skill targets must not be empty",
            1,
        ));
    }
    if tokens.iter().any(|value| value == "all") {
        return Ok(vec![
            SkillTarget::Claude,
            SkillTarget::Codex,
            SkillTarget::Copilot,
            SkillTarget::Opencode,
        ]);
    }

    let mut unique = Vec::new();
    for token in tokens {
        let target = match token.as_str() {
            "claude" => SkillTarget::Claude,
            "codex" => SkillTarget::Codex,
            "copilot" => SkillTarget::Copilot,
            "opencode" => SkillTarget::Opencode,
            _ => {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "skill targets must be comma-separated values of claude,codex,copilot,opencode,all",
                    1,
                ));
            }
        };
        if !unique.contains(&target) {
            unique.push(target);
        }
    }
    Ok(unique)
}

pub fn parse_dep_direction(raw: Option<&str>) -> Result<Option<DepDirection>, TsqError> {
    match raw {
        None => Ok(None),
        Some("up") => Ok(Some(DepDirection::Up)),
        Some("down") => Ok(Some(DepDirection::Down)),
        Some("both") => Ok(Some(DepDirection::Both)),
        Some(_) => Err(TsqError::new(
            "VALIDATION_ERROR",
            "direction must be up|down|both",
            1,
        )),
    }
}

pub fn parse_dependency_type(raw: &str) -> Result<DependencyType, TsqError> {
    match raw {
        "blocks" => Ok(DependencyType::Blocks),
        "starts_after" => Ok(DependencyType::StartsAfter),
        _ => Err(TsqError::new(
            "VALIDATION_ERROR",
            "dependency type must be blocks|starts_after",
            1,
        )),
    }
}

pub fn parse_dep_filter_direction(raw: &str) -> Result<DepDirectionFilter, TsqError> {
    match raw {
        "in" => Ok(DepDirectionFilter::In),
        "out" => Ok(DepDirectionFilter::Out),
        "any" => Ok(DepDirectionFilter::Any),
        _ => Err(TsqError::new(
            "VALIDATION_ERROR",
            "dep-direction must be in|out|any",
            1,
        )),
    }
}

pub fn parse_non_negative_int(raw: &str, field: &str) -> Result<i64, TsqError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.chars().all(|char| char.is_ascii_digit()) {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            format!("{} must be an integer >= 0", field),
            1,
        ));
    }
    trimmed.parse::<i64>().map_err(|_| {
        TsqError::new(
            "VALIDATION_ERROR",
            format!("{} must be an integer >= 0", field),
            1,
        )
    })
}

pub fn parse_positive_int(raw: &str, field: &str, min: i64, max: i64) -> Result<i64, TsqError> {
    let value = parse_non_negative_int(raw, field)?;
    if value < min || value > max {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            format!("{} must be between {} and {}", field, min, max),
            1,
        ));
    }
    Ok(value)
}

pub fn parse_list_filter(input: ListParseInput) -> Result<ListFilter, TsqError> {
    let mut filter = ListFilter {
        statuses: None,
        assignee: None,
        external_ref: None,
        discovered_from: None,
        kind: None,
        label: None,
        label_any: None,
        created_after: None,
        updated_after: None,
        closed_after: None,
        unassigned: false,
        ids: None,
        planning_state: None,
        dep_type: None,
        dep_direction: None,
    };

    if let Some(status) = input.status.as_deref() {
        filter.statuses = Some(vec![normalize_status(status)?]);
    }
    if input.unassigned && input.has_assignee_flag {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "cannot combine --assignee with --unassigned",
            1,
        ));
    }
    if let Some(assignee) = as_optional_string(input.assignee.as_deref()) {
        filter.assignee = Some(assignee);
    }
    if let Some(external_ref) = as_optional_string(input.external_ref.as_deref()) {
        filter.external_ref = Some(external_ref);
    }
    if let Some(discovered_from) = as_optional_string(input.discovered_from.as_deref()) {
        filter.discovered_from = Some(discovered_from);
    }
    if let Some(kind) = input.kind.as_deref() {
        filter.kind = Some(parse_kind(kind)?);
    }
    if let Some(label) = as_optional_string(input.label.as_deref()) {
        filter.label = Some(label);
    }
    if let Some(label_any) = parse_repeatable_csv_values(input.label_any, "label-any")? {
        filter.label_any = Some(unique_sorted(label_any));
    }
    if let Some(created_after) = input.created_after.as_deref() {
        filter.created_after = Some(parse_iso_timestamp(created_after, "created-after")?);
    }
    if let Some(updated_after) = input.updated_after.as_deref() {
        filter.updated_after = Some(parse_iso_timestamp(updated_after, "updated-after")?);
    }
    if let Some(closed_after) = input.closed_after.as_deref() {
        filter.closed_after = Some(parse_iso_timestamp(closed_after, "closed-after")?);
    }
    if input.unassigned {
        filter.unassigned = true;
    }
    if let Some(ids) = parse_repeatable_csv_values(input.ids, "id")? {
        filter.ids = Some(unique_sorted(ids));
    }
    if let Some(planning) = input.planning.as_deref() {
        filter.planning_state = Some(parse_planning_state(planning)?);
    }

    if let Some(dep_type) = input.dep_type.as_deref() {
        filter.dep_type = Some(parse_dependency_type(dep_type)?);
        filter.dep_direction = Some(match input.dep_direction.as_deref() {
            Some(raw) => parse_dep_filter_direction(raw)?,
            None => DepDirectionFilter::Any,
        });
    } else if input.dep_direction.is_some() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "--dep-direction requires --dep-type",
            1,
        ));
    }

    Ok(filter)
}

pub fn apply_tree_defaults(filter: ListFilter, full: bool) -> ListFilter {
    if full || filter.statuses.is_some() {
        return filter;
    }
    ListFilter {
        statuses: Some(TREE_DEFAULT_STATUSES.to_vec()),
        ..filter
    }
}

pub fn parse_status_csv(raw: &str) -> Result<Vec<TaskStatus>, TsqError> {
    let mut statuses = Vec::new();
    for token in raw.split(',') {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        let status = normalize_status(trimmed)?;
        statuses.push(status);
    }
    if statuses.is_empty() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "status must not be empty",
            1,
        ));
    }
    Ok(statuses)
}

pub fn parse_priority_value(raw: &str) -> Result<u8, TsqError> {
    parse_priority(raw)
}

fn parse_iso_timestamp(raw: &str, field: &str) -> Result<String, TsqError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !ISO_PATTERN.is_match(trimmed) {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            format!("--{} must be a valid ISO timestamp", field),
            1,
        ));
    }
    match chrono::DateTime::parse_from_rfc3339(trimmed) {
        Ok(value) => Ok(value
            .to_utc()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        Err(_) => Err(TsqError::new(
            "VALIDATION_ERROR",
            format!("--{} must be a valid ISO timestamp", field),
            1,
        )),
    }
}

fn parse_repeatable_csv_values(
    raw_values: Vec<String>,
    field: &str,
) -> Result<Option<Vec<String>>, TsqError> {
    if raw_values.is_empty() {
        return Ok(None);
    }
    let normalized: Vec<String> = raw_values
        .into_iter()
        .map(|value| value.trim().to_string())
        .collect();
    if normalized.iter().all(|value| value.is_empty()) {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            format!("--{} must not be empty", field),
            1,
        ));
    }
    if normalized.iter().any(|value| value.is_empty()) {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            format!("--{} values must not be empty", field),
            1,
        ));
    }
    Ok(Some(normalized))
}

fn unique_sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}
