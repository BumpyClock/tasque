use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparsingFormat {
    Human,
    Json,
    Plain,
}

#[derive(Debug, Clone, Default)]
pub struct PreparseResult {
    pub json: bool,
    pub plain: bool,
    pub exact_id: bool,
    pub format: Option<PreparsingFormat>,
    pub root: Option<PathBuf>,
    pub command_index: Option<usize>,
    pub command: Option<String>,
    pub display_only: bool,
}

impl PreparseResult {
    pub fn wants_json(&self) -> bool {
        self.json || matches!(self.format, Some(PreparsingFormat::Json))
    }

    pub fn explicit_root(&self) -> bool {
        self.root.is_some()
    }
}

pub fn preparse_args(args: &[String]) -> PreparseResult {
    let mut result = PreparseResult::default();
    let mut index = 1;
    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "--" => break,
            "--json" => result.json = true,
            "--plain" => result.plain = true,
            "--exact-id" => result.exact_id = true,
            "--help" | "-h" | "--version" | "-V" => result.display_only = true,
            "help" if result.command.is_none() => result.display_only = true,
            "--root" => {
                if let Some(value) = option_value(args, index) {
                    result.root = Some(PathBuf::from(value));
                    index += 1;
                }
            }
            "--format" => {
                if let Some(value) = option_value(args, index) {
                    result.format = parse_format(value);
                    index += 1;
                }
            }
            value if value.starts_with("--root=") => {
                if let Some(value) = value.strip_prefix("--root=")
                    && !value.is_empty()
                {
                    result.root = Some(PathBuf::from(value));
                }
            }
            value if value.starts_with("--format=") => {
                result.format = value.strip_prefix("--format=").and_then(parse_format);
            }
            value if value.starts_with('-') => {}
            value => {
                if result.command.is_none() {
                    result.command_index = Some(index);
                    result.command = Some(value.to_string());
                }
            }
        }
        index += 1;
    }
    result
}

fn option_value(args: &[String], index: usize) -> Option<&str> {
    args.get(index + 1)
        .map(String::as_str)
        .filter(|value| !value.starts_with('-'))
}

fn parse_format(value: &str) -> Option<PreparsingFormat> {
    match value {
        "human" => Some(PreparsingFormat::Human),
        "json" => Some(PreparsingFormat::Json),
        "plain" => Some(PreparsingFormat::Plain),
        _ => None,
    }
}
