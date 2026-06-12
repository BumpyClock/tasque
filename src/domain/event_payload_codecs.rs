use crate::types::{DependencyType, EventType, PlanningState, RelationType, TaskKind, TaskStatus};
use serde::{Serialize, de::DeserializeOwned};

pub fn event_type_from_str(raw: &str) -> Option<EventType> {
    deserialize_wire_name(raw)
}

pub fn event_type_as_str(event_type: EventType) -> String {
    serialize_wire_name(event_type)
}

pub fn task_kind_from_str(raw: &str) -> Option<TaskKind> {
    deserialize_wire_name(raw)
}

pub fn task_kind_as_str(kind: TaskKind) -> String {
    serialize_wire_name(kind)
}

pub fn task_status_from_str(raw: &str) -> Option<TaskStatus> {
    deserialize_wire_name(raw)
}

pub fn task_status_as_str(status: TaskStatus) -> String {
    serialize_wire_name(status)
}

pub fn planning_state_from_str(raw: &str) -> Option<PlanningState> {
    deserialize_wire_name(raw)
}

pub fn planning_state_as_str(state: PlanningState) -> String {
    serialize_wire_name(state)
}

pub fn dependency_type_from_str(raw: &str) -> Option<DependencyType> {
    deserialize_wire_name(raw)
}

pub fn dependency_type_as_str(dep_type: DependencyType) -> String {
    serialize_wire_name(dep_type)
}

pub fn relation_type_from_str(raw: &str) -> Option<RelationType> {
    deserialize_wire_name(raw)
}

pub fn relation_type_as_str(rel_type: RelationType) -> String {
    serialize_wire_name(rel_type)
}

fn deserialize_wire_name<T>(raw: &str) -> Option<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value(serde_json::Value::String(raw.to_string())).ok()
}

fn serialize_wire_name<T>(value: T) -> String
where
    T: Serialize,
{
    match serde_json::to_value(value).expect("enum wire name must serialize") {
        serde_json::Value::String(name) => name,
        other => panic!("enum wire name serialized to non-string value: {other}"),
    }
}
