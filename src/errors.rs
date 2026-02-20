use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone)]
pub struct TsqError {
    pub code: String,
    pub message: String,
    pub exit_code: i32,
    pub details: Option<Value>,
}

impl TsqError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, exit_code: i32) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            exit_code,
            details: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
}

impl fmt::Display for TsqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for TsqError {}
