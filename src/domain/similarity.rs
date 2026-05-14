use crate::types::{Task, TaskStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const DEFAULT_SIMILARITY_MIN_SCORE: f64 = 0.35;
pub const DEFAULT_SIMILARITY_LIMIT: usize = 10;
pub const BLOCKING_DUPLICATE_THRESHOLD: f64 = 0.8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarTaskCandidate {
    pub task: Task,
    pub score: f64,
    pub reason: String,
}

pub fn blocking_status(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Open | TaskStatus::InProgress | TaskStatus::Blocked | TaskStatus::Deferred
    )
}

pub fn find_similar_candidates<'a>(
    tasks: impl IntoIterator<Item = &'a Task>,
    input: &str,
    min_score: f64,
    limit: usize,
) -> Vec<SimilarTaskCandidate> {
    let mut candidates = tasks
        .into_iter()
        .filter_map(|task| score_task(input, task).map(|(score, reason)| (task, score, reason)))
        .filter(|(_, score, _)| *score >= min_score)
        .map(|(task, score, reason)| SimilarTaskCandidate {
            task: task.clone(),
            score,
            reason,
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.task.priority.cmp(&b.task.priority))
            .then_with(|| a.task.created_at.cmp(&b.task.created_at))
            .then_with(|| a.task.id.cmp(&b.task.id))
    });
    candidates.truncate(limit);
    candidates
}

pub fn is_blocking_duplicate(input: &str, task: &Task) -> Option<SimilarTaskCandidate> {
    let (score, reason) = score_task(input, task)?;
    if score >= BLOCKING_DUPLICATE_THRESHOLD
        || reason == "normalized_title_exact"
        || reason == "alias_exact"
    {
        Some(SimilarTaskCandidate {
            task: task.clone(),
            score,
            reason,
        })
    } else {
        None
    }
}

/// Check if two raw title strings are similar enough to be blocking duplicates.
/// Returns `Some((score, reason))` if they are, `None` otherwise.
pub fn is_blocking_title_pair(a: &str, b: &str) -> Option<(f64, String)> {
    let (score, reason) = score_titles(a, b)?;
    if score >= BLOCKING_DUPLICATE_THRESHOLD || reason == "normalized_title_exact" {
        Some((score, reason))
    } else {
        None
    }
}

pub fn normalized_text(input: &str) -> String {
    let chars = input.to_lowercase().chars().collect::<Vec<_>>();
    chars
        .iter()
        .enumerate()
        .map(|(index, ch)| {
            if ch.is_ascii_alphanumeric() || *ch == '#' {
                return *ch;
            }
            if matches!(ch, '.' | '-') {
                let prev_is_digit = index
                    .checked_sub(1)
                    .and_then(|prev| chars.get(prev))
                    .map(|prev| prev.is_ascii_digit())
                    .unwrap_or(false);
                let next_is_digit = chars
                    .get(index + 1)
                    .map(|next| next.is_ascii_digit())
                    .unwrap_or(false);
                if prev_is_digit || next_is_digit {
                    return *ch;
                }
            }
            ' '
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn score_task(input: &str, task: &Task) -> Option<(f64, String)> {
    let input_norm = normalized_text(input);
    if input_norm.is_empty() {
        return None;
    }
    let title_norm = normalized_text(&task.title);
    if input_norm == title_norm {
        return Some((1.0, "normalized_title_exact".to_string()));
    }
    let input_alias = input_norm.replace(' ', "-");
    if input_alias == task.alias {
        return Some((1.0, "alias_exact".to_string()));
    }
    if task.alias.starts_with(&input_alias) {
        return Some((0.95, "alias_prefix".to_string()));
    }
    score_normalized_titles(&input_norm, &title_norm)
}

fn score_titles(a: &str, b: &str) -> Option<(f64, String)> {
    let a_norm = normalized_text(a);
    if a_norm.is_empty() {
        return None;
    }
    let b_norm = normalized_text(b);
    if a_norm == b_norm {
        return Some((1.0, "normalized_title_exact".to_string()));
    }
    score_normalized_titles(&a_norm, &b_norm)
}

fn score_normalized_titles(input_norm: &str, title_norm: &str) -> Option<(f64, String)> {
    // Only treat substring containment as a phrase match when the contained
    // string has at least 2 meaningful tokens. This prevents single-char or
    // single-word titles (e.g. "A") from matching longer titles (e.g. "Parent")
    // via trivial substring hits.
    if title_norm.contains(input_norm) || input_norm.contains(title_norm) {
        let contained = if title_norm.contains(input_norm) {
            input_norm
        } else {
            title_norm
        };
        if meaningful_tokens(contained).len() >= 2 {
            return Some((0.9, "title_phrase".to_string()));
        }
    }
    let input_tokens = meaningful_tokens(input_norm);
    let title_tokens = meaningful_tokens(title_norm);
    let overlap = token_overlap(&input_tokens, &title_tokens)?;
    if overlap >= 0.8 {
        return Some((overlap, "title_token_containment".to_string()));
    }
    if overlap >= 0.35 {
        return Some((overlap, "title_token_overlap".to_string()));
    }
    None
}

fn meaningful_tokens(input: &str) -> HashSet<String> {
    const STOPWORDS: &[&str] = &["a", "an", "the", "of", "to", "for", "and", "or", "in", "on"];
    input
        .split_whitespace()
        .filter(|token| token.len() >= 3 && !STOPWORDS.contains(token))
        .map(stem_plural)
        .collect()
}

/// Normalize simple English plurals so "warning" and "warnings" match.
fn stem_plural(token: &str) -> String {
    if token.len() > 3 && token.ends_with('s') && !token.ends_with("ss") {
        token[..token.len() - 1].to_string()
    } else {
        token.to_string()
    }
}

fn token_overlap(left: &HashSet<String>, right: &HashSet<String>) -> Option<f64> {
    let min_len = left.len().min(right.len());
    if min_len < 2 {
        return None;
    }
    let intersection = left.intersection(right).count();
    Some(intersection as f64 / min_len as f64)
}

#[cfg(test)]
mod tests {
    use super::normalized_text;

    #[test]
    fn normalized_text_preserves_issue_and_version_tokens() {
        assert_eq!(
            normalized_text("Fix #123 in v1.2.3 and GH-456"),
            "fix #123 in v1.2.3 and gh-456"
        );
    }
}
