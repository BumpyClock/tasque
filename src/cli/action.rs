use crate::errors::TsqError;
use crate::output::{err_envelope, ok_envelope};
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub struct GlobalOpts {
    pub json: bool,
    pub exact_id: bool,
}

pub fn run_action<T, J, F, M, H>(
    command_line: &str,
    opts: GlobalOpts,
    action: F,
    map_json: M,
    human: H,
) -> i32
where
    F: FnOnce() -> Result<T, TsqError>,
    M: FnOnce(&T) -> J,
    H: FnOnce(&T) -> Result<(), TsqError>,
    J: Serialize,
{
    match action() {
        Ok(value) => {
            if opts.json {
                let envelope = ok_envelope(command_line, map_json(&value));
                match serde_json::to_string_pretty(&envelope) {
                    Ok(text) => println!("{}", text),
                    Err(error) => {
                        eprintln!("INTERNAL_ERROR: failed serializing json output: {}", error);
                        return 2;
                    }
                }
            } else if let Err(error) = human(&value) {
                eprintln!("{}: {}", error.code, error.message);
                if let Some(details) = error.details {
                    eprintln!("{}", details);
                }
                return error.exit_code;
            }
            0
        }
        Err(error) => {
            if opts.json {
                let envelope = err_envelope(
                    command_line,
                    error.code.clone(),
                    error.message.clone(),
                    error.details.clone(),
                );
                match serde_json::to_string_pretty(&envelope) {
                    Ok(text) => println!("{}", text),
                    Err(json_error) => {
                        eprintln!(
                            "INTERNAL_ERROR: failed serializing error envelope: {}",
                            json_error
                        );
                        return 2;
                    }
                }
            } else {
                eprintln!("{}: {}", error.code, error.message);
                if let Some(details) = error.details {
                    eprintln!("{}", details);
                }
            }
            error.exit_code
        }
    }
}

pub fn emit_error(command_line: &str, opts: GlobalOpts, error: TsqError) -> i32 {
    if opts.json {
        let envelope = err_envelope(
            command_line,
            error.code.clone(),
            error.message.clone(),
            error.details.clone(),
        );
        match serde_json::to_string_pretty(&envelope) {
            Ok(text) => println!("{}", text),
            Err(json_error) => {
                eprintln!(
                    "INTERNAL_ERROR: failed serializing error envelope: {}",
                    json_error
                );
                return 2;
            }
        }
    } else {
        eprintln!("{}: {}", error.code, error.message);
        if let Some(details) = error.details {
            eprintln!("{}", details);
        }
    }
    error.exit_code
}
