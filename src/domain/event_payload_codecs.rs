use crate::types::{DependencyType, EventType, PlanningState, RelationType, TaskKind, TaskStatus};

pub fn event_type_from_str(raw: &str) -> Option<EventType> {
    match raw {
        "task.created" => Some(EventType::TaskCreated),
        "task.updated" => Some(EventType::TaskUpdated),
        "task.status_set" => Some(EventType::TaskStatusSet),
        "task.claimed" => Some(EventType::TaskClaimed),
        "task.noted" => Some(EventType::TaskNoted),
        "task.spec_attached" => Some(EventType::TaskSpecAttached),
        "task.superseded" => Some(EventType::TaskSuperseded),
        "dep.added" => Some(EventType::DepAdded),
        "dep.removed" => Some(EventType::DepRemoved),
        "link.added" => Some(EventType::LinkAdded),
        "link.removed" => Some(EventType::LinkRemoved),
        _ => None,
    }
}

pub fn event_type_as_str(event_type: EventType) -> &'static str {
    match event_type {
        EventType::TaskCreated => "task.created",
        EventType::TaskUpdated => "task.updated",
        EventType::TaskStatusSet => "task.status_set",
        EventType::TaskClaimed => "task.claimed",
        EventType::TaskNoted => "task.noted",
        EventType::TaskSpecAttached => "task.spec_attached",
        EventType::TaskSuperseded => "task.superseded",
        EventType::DepAdded => "dep.added",
        EventType::DepRemoved => "dep.removed",
        EventType::LinkAdded => "link.added",
        EventType::LinkRemoved => "link.removed",
    }
}

pub fn task_kind_from_str(raw: &str) -> Option<TaskKind> {
    match raw {
        "task" => Some(TaskKind::Task),
        "feature" => Some(TaskKind::Feature),
        "epic" => Some(TaskKind::Epic),
        _ => None,
    }
}

pub fn task_status_from_str(raw: &str) -> Option<TaskStatus> {
    match raw {
        "open" => Some(TaskStatus::Open),
        "in_progress" => Some(TaskStatus::InProgress),
        "blocked" => Some(TaskStatus::Blocked),
        "closed" => Some(TaskStatus::Closed),
        "canceled" => Some(TaskStatus::Canceled),
        "deferred" => Some(TaskStatus::Deferred),
        _ => None,
    }
}

pub fn task_status_as_str(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Open => "open",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Closed => "closed",
        TaskStatus::Canceled => "canceled",
        TaskStatus::Deferred => "deferred",
    }
}

pub fn planning_state_from_str(raw: &str) -> Option<PlanningState> {
    match raw {
        "needs_planning" => Some(PlanningState::NeedsPlanning),
        "planned" => Some(PlanningState::Planned),
        _ => None,
    }
}

pub fn dependency_type_from_str(raw: &str) -> Option<DependencyType> {
    match raw {
        "blocks" => Some(DependencyType::Blocks),
        "starts_after" => Some(DependencyType::StartsAfter),
        _ => None,
    }
}

pub fn relation_type_from_str(raw: &str) -> Option<RelationType> {
    match raw {
        "relates_to" => Some(RelationType::RelatesTo),
        "replies_to" => Some(RelationType::RepliesTo),
        "duplicates" => Some(RelationType::Duplicates),
        "supersedes" => Some(RelationType::Supersedes),
        _ => None,
    }
}
