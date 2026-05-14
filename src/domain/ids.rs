use crate::errors::TsqError;
use crate::types::State;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;

static SEQUENTIAL_ROOT_ID: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^tsq-[1-9][0-9]*$").expect("sequential root id regex"));

pub fn make_root_id(state: &State) -> Result<String, TsqError> {
    let mut next = next_root_number(state)?;
    loop {
        let candidate = format!("tsq-{}", next);
        if !state.tasks.contains_key(&candidate) {
            return Ok(candidate);
        }
        next = next.checked_add(1).ok_or_else(id_overflow_error)?;
    }
}

pub struct RootIdAllocator {
    next: u64,
    reserved_ids: HashSet<String>,
}

impl RootIdAllocator {
    pub fn new(state: &State) -> Result<Self, TsqError> {
        Ok(Self {
            next: next_root_number(state)?,
            reserved_ids: state.tasks.keys().cloned().collect(),
        })
    }

    pub fn next_id(&mut self) -> Result<String, TsqError> {
        loop {
            let candidate = format!("tsq-{}", self.next);
            self.next = self.next.checked_add(1).ok_or_else(id_overflow_error)?;
            if self.reserved_ids.insert(candidate.clone()) {
                return Ok(candidate);
            }
        }
    }
}

fn next_root_number(state: &State) -> Result<u64, TsqError> {
    let mut max_seen = 0u64;
    for task in state.tasks.values() {
        if task.parent_id.is_none() {
            if let Some(number) = sequential_number(&task.id) {
                max_seen = max_seen.max(number);
            }
        }
    }
    max_seen.checked_add(1).ok_or_else(id_overflow_error)
}

fn id_overflow_error() -> TsqError {
    TsqError::new("ID_OVERFLOW", "unable to allocate sequential task id", 2)
}

pub fn is_valid_root_id(raw: &str) -> bool {
    is_sequential_root_id(raw) || is_legacy_random_root_id(raw)
}

pub fn is_sequential_root_id(raw: &str) -> bool {
    SEQUENTIAL_ROOT_ID.is_match(raw)
}

pub fn is_legacy_random_root_id(raw: &str) -> bool {
    let Some(rest) = raw.strip_prefix("tsq-") else {
        return false;
    };
    rest.len() == 8
        && rest
            .chars()
            .all(|ch| matches!(ch, '0'..='9' | 'a'..='h' | 'j'..='k' | 'm'..='n' | 'p'..='t' | 'v'..='z'))
}

pub fn next_child_id(state: &State, parent_id: &str) -> String {
    let max_child = state.child_counters.get(parent_id).copied().unwrap_or(0);
    format!("{}.{}", parent_id, max_child + 1)
}

fn sequential_number(raw: &str) -> Option<u64> {
    let suffix = raw.strip_prefix("tsq-")?;
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    suffix.parse::<u64>().ok()
}
