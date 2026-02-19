use crate::types::{DependencyEdge, DependencyType};
use std::collections::HashSet;

pub fn normalize_dependency_type(value: &str) -> Option<DependencyType> {
    match value {
        "blocks" => Some(DependencyType::Blocks),
        "starts_after" => Some(DependencyType::StartsAfter),
        _ => None,
    }
}

pub fn normalize_dependency_edges(edges: Option<&Vec<DependencyEdge>>) -> Vec<DependencyEdge> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    let Some(edges) = edges else {
        return normalized;
    };
    for edge in edges {
        if edge.blocker.is_empty() {
            continue;
        }
        let key = edge_key(&edge.blocker, edge.dep_type);
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);
        normalized.push(edge.clone());
    }
    normalized
}

pub fn edge_key(blocker: &str, dep_type: DependencyType) -> String {
    let dep_value = match dep_type {
        DependencyType::Blocks => "blocks",
        DependencyType::StartsAfter => "starts_after",
    };
    format!("{blocker}\u{0000}{dep_value}")
}
