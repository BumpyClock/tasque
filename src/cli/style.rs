use crate::types::TaskStatus;
use std::env;
use std::io::IsTerminal;

const ANSI_RESET: &str = "\x1b[0m";

pub fn use_color() -> bool {
    if let Ok(force) = env::var("CLICOLOR_FORCE")
        && force != "0"
    {
        return true;
    }
    if env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if matches!(env::var("CLICOLOR").as_deref(), Ok("0")) {
        return false;
    }
    std::io::stdout().is_terminal()
}

pub fn heading(value: &str) -> String {
    paint(value, "1;36")
}

pub fn key(value: &str) -> String {
    paint(value, "36")
}

pub fn task_id(value: &str) -> String {
    paint(value, "1;94")
}

pub fn tree_prefix(value: &str) -> String {
    paint(value, "90")
}

pub fn meta(value: &str) -> String {
    paint(value, "35")
}

pub fn flow(value: &str) -> String {
    paint(value, "36")
}

pub fn warning(value: &str) -> String {
    paint(value, "1;33")
}

pub fn success(value: &str) -> String {
    paint(value, "1;32")
}

pub fn error(value: &str) -> String {
    paint(value, "1;31")
}

pub fn muted(value: &str) -> String {
    paint(value, "90")
}

pub fn status(value: &str, status: TaskStatus) -> String {
    let code = match status {
        TaskStatus::Open => "1;34",
        TaskStatus::InProgress => "1;36",
        TaskStatus::Blocked => "1;31",
        TaskStatus::Closed => "1;32",
        TaskStatus::Canceled => "90",
        TaskStatus::Deferred => "1;33",
    };
    paint(value, code)
}

fn paint(value: &str, code: &str) -> String {
    if !use_color() {
        return value.to_string();
    }
    format!("\x1b[{}m{}{}", code, value, ANSI_RESET)
}
