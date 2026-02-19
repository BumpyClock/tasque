use crate::errors::TsqError;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;

const MAX_LABEL_LENGTH: usize = 64;
static LABEL_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z0-9:_/\-]+$").expect("valid label pattern"));

pub fn normalize_label(raw: &str) -> Result<String, TsqError> {
    let trimmed = raw.trim().to_lowercase();
    if trimmed.is_empty() {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "label must not be empty",
            1,
        ));
    }
    if trimmed.len() > MAX_LABEL_LENGTH {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            format!("label must not exceed {} characters", MAX_LABEL_LENGTH),
            1,
        ));
    }
    if !LABEL_PATTERN.is_match(&trimmed) {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            "label must only contain characters [a-z0-9:_/-]",
            1,
        ));
    }
    Ok(trimmed)
}

pub fn add_label(current: &[String], label: &str) -> Result<Vec<String>, TsqError> {
    let normalized = normalize_label(label)?;
    let mut set: HashSet<String> = current.iter().cloned().collect();
    set.insert(normalized);
    let mut next: Vec<String> = set.into_iter().collect();
    next.sort();
    Ok(next)
}

pub fn remove_label(current: &[String], label: &str) -> Result<Vec<String>, TsqError> {
    let normalized = normalize_label(label)?;
    if !current.iter().any(|entry| entry == &normalized) {
        return Err(TsqError::new(
            "NOT_FOUND",
            format!("label not found: {}", normalized),
            1,
        ));
    }
    Ok(current
        .iter()
        .filter(|entry| *entry != &normalized)
        .cloned()
        .collect())
}
