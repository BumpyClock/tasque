use crate::domain::dep_tree::build_dependents_by_blocker;
use crate::domain::deps::{normalize_dependency_edges, normalize_dependency_type};
use crate::domain::validate::is_ready;
use crate::errors::TsqError;
use crate::types::{State, Task, TaskKind, TaskStatus};

/// A single parsed search term with optional field qualifier and negation.
/// Example: status:open becomes field="status" value="open" negated=false.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryTerm {
    pub field: String,
    pub value: String,
    pub negated: bool,
}

/// A structured query filter produced by parse_query.
/// Example: parse_query("status:open label:bug") yields two terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryFilter {
    pub terms: Vec<QueryTerm>,
}

/// Parse a query string into structured terms.
/// Example: parse_query("title:\"my task\"") returns a title term.
pub fn parse_query(q: &str) -> Result<QueryFilter, TsqError> {
    let tokens = tokenize(q.trim());
    let mut terms: Vec<QueryTerm> = Vec::new();
    let mut bare_words: Vec<String> = Vec::new();

    for token in tokens {
        if let Some((negated, raw_field, raw_value)) = parse_field_term(&token) {
            if !bare_words.is_empty() {
                terms.push(QueryTerm {
                    field: "text".to_string(),
                    value: bare_words.join(" "),
                    negated: false,
                });
                bare_words.clear();
            }
            if raw_field == "dep_type" {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "dep_type requires explicit direction; use dep_type_in:<type> or dep_type_out:<type>",
                    1,
                ));
            }
            let value = unquote(raw_value);
            let field = if is_supported_field(raw_field) {
                raw_field.to_string()
            } else {
                "text".to_string()
            };
            if (field == "dep_type_in" || field == "dep_type_out")
                && normalize_dependency_type(value.as_str()).is_none()
            {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    format!("{} must be blocks|starts_after", field),
                    1,
                ));
            }
            let term_value = if field == "text" && !is_supported_field(raw_field) {
                format!("{}:{}", raw_field, value)
            } else {
                value
            };
            terms.push(QueryTerm {
                field,
                value: term_value,
                negated,
            });
        } else {
            bare_words.push(unquote(&token));
        }
    }

    if !bare_words.is_empty() {
        terms.push(QueryTerm {
            field: "text".to_string(),
            value: bare_words.join(" "),
            negated: false,
        });
    }

    Ok(QueryFilter { terms })
}

/// Evaluate a query filter against tasks using implicit AND logic.
/// Example: evaluate_query(tasks, &filter, state).
pub fn evaluate_query(tasks: &[Task], filter: &QueryFilter, state: &State) -> Vec<Task> {
    if filter.terms.is_empty() {
        return tasks.to_vec();
    }
    tasks
        .iter()
        .filter(|&task| matches_all(task, &filter.terms, state))
        .cloned()
        .collect()
}

fn matches_all(task: &Task, terms: &[QueryTerm], state: &State) -> bool {
    for term in terms {
        let matched = match_term(task, term, state);
        if term.negated {
            if matched {
                return false;
            }
        } else if !matched {
            return false;
        }
    }
    true
}

fn match_term(task: &Task, term: &QueryTerm, state: &State) -> bool {
    match term.field.as_str() {
        "id" => task.id == term.value || task.id.starts_with(&term.value),
        "text" => match_task_text(task, &term.value),
        "title" => task
            .title
            .to_lowercase()
            .contains(&term.value.to_lowercase()),
        "description" => task
            .description
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains(&term.value.to_lowercase()),
        "notes" => task.notes.iter().any(|note| {
            note.text
                .to_lowercase()
                .contains(&term.value.to_lowercase())
        }),
        "status" => matches_status(task.status, &term.value),
        "kind" => matches_kind(task.kind, &term.value),
        "priority" => task.priority.to_string() == term.value,
        "assignee" => task.assignee.as_deref() == Some(term.value.as_str()),
        "external_ref" => task.external_ref.as_deref() == Some(term.value.as_str()),
        "discovered_from" => task.discovered_from.as_deref() == Some(term.value.as_str()),
        "parent" => task.parent_id.as_deref() == Some(term.value.as_str()),
        "label" => task
            .labels
            .iter()
            .any(|label| label.to_lowercase() == term.value.to_lowercase()),
        "ready" => is_ready(state, &task.id) == (term.value == "true"),
        "dep_type_in" => has_incoming_dep_type(state, &task.id, &term.value),
        "dep_type_out" => has_outgoing_dep_type(state, &task.id, &term.value),
        _ => match_task_text(task, &term.value),
    }
}

fn matches_status(status: TaskStatus, value: &str) -> bool {
    match value {
        "done" => status == TaskStatus::Closed,
        "todo" => status == TaskStatus::Open,
        "open" => status == TaskStatus::Open,
        "in_progress" => status == TaskStatus::InProgress,
        "blocked" => status == TaskStatus::Blocked,
        "closed" => status == TaskStatus::Closed,
        "canceled" => status == TaskStatus::Canceled,
        "deferred" => status == TaskStatus::Deferred,
        _ => false,
    }
}

fn matches_kind(kind: TaskKind, value: &str) -> bool {
    match value {
        "task" => kind == TaskKind::Task,
        "feature" => kind == TaskKind::Feature,
        "epic" => kind == TaskKind::Epic,
        _ => false,
    }
}

fn has_outgoing_dep_type(state: &State, task_id: &str, raw_type: &str) -> bool {
    let dep_type = match normalize_dependency_type(raw_type) {
        Some(dep_type) => dep_type,
        None => return false,
    };
    normalize_dependency_edges(state.deps.get(task_id))
        .iter()
        .any(|edge| edge.dep_type == dep_type)
}

fn has_incoming_dep_type(state: &State, task_id: &str, raw_type: &str) -> bool {
    let dep_type = match normalize_dependency_type(raw_type) {
        Some(dep_type) => dep_type,
        None => return false,
    };
    let dependents = build_dependents_by_blocker(&state.deps);
    dependents
        .get(task_id)
        .map(|edges| edges.iter().any(|edge| edge.dep_type == dep_type))
        .unwrap_or(false)
}

fn match_task_text(task: &Task, value: &str) -> bool {
    let needle = value.to_lowercase();
    if task.title.to_lowercase().contains(&needle) {
        return true;
    }
    if task
        .description
        .as_deref()
        .unwrap_or("")
        .to_lowercase()
        .contains(&needle)
    {
        return true;
    }
    task.notes
        .iter()
        .any(|note| note.text.to_lowercase().contains(&needle))
}

fn tokenize(input: &str) -> Vec<String> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens: Vec<String> = Vec::new();
    let mut idx = 0;

    while idx < chars.len() {
        let ch = chars[idx];
        if ch == ' ' || ch == '\t' {
            idx += 1;
            continue;
        }

        if ch == '"' {
            let mut end = idx + 1;
            while end < chars.len() && chars[end] != '"' {
                end += 1;
            }
            if end >= chars.len() {
                tokens.push(chars[idx + 1..].iter().collect());
                break;
            }
            tokens.push(chars[idx..=end].iter().collect());
            idx = end + 1;
            continue;
        }

        let mut end = idx;
        while end < chars.len() && chars[end] != ' ' && chars[end] != '\t' {
            if chars[end] == '"' {
                let mut close_quote = end + 1;
                while close_quote < chars.len() && chars[close_quote] != '"' {
                    close_quote += 1;
                }
                end = if close_quote >= chars.len() {
                    chars.len()
                } else {
                    close_quote + 1
                };
            } else {
                end += 1;
            }
        }
        tokens.push(chars[idx..end].iter().collect());
        idx = end;
    }

    tokens
}

fn parse_field_term(token: &str) -> Option<(bool, &str, &str)> {
    let (negated, rest) = if let Some(stripped) = token.strip_prefix('-') {
        (true, stripped)
    } else {
        (false, token)
    };
    let colon_idx = rest.find(':')?;
    let field = &rest[..colon_idx];
    if field.is_empty() {
        return None;
    }
    if !field
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    let value = &rest[colon_idx + 1..];
    if value.is_empty() {
        return None;
    }
    Some((negated, field, value))
}

fn unquote(value: &str) -> String {
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        return value[1..value.len() - 1].to_string();
    }
    value.to_string()
}

fn is_supported_field(field: &str) -> bool {
    matches!(
        field,
        "id" | "text"
            | "title"
            | "description"
            | "notes"
            | "status"
            | "kind"
            | "priority"
            | "assignee"
            | "external_ref"
            | "discovered_from"
            | "parent"
            | "label"
            | "ready"
            | "dep_type_in"
            | "dep_type_out"
    )
}
