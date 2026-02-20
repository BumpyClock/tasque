use crate::errors::TsqError;
use std::io::Read;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

const STDIN_TIMEOUT_MS: u64 = 30_000;

pub fn read_stdin_content() -> Result<String, TsqError> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut input = String::new();
        let result = std::io::stdin().read_to_string(&mut input).map(|_| input);
        let _ = tx.send(result);
    });

    match rx.recv_timeout(Duration::from_millis(STDIN_TIMEOUT_MS)) {
        Ok(result) => {
            let content = result.map_err(|error| {
                TsqError::new("IO_ERROR", "failed reading stdin", 2)
                    .with_details(serde_json::json!({"message": error.to_string()}))
            })?;
            if content.trim().is_empty() {
                return Err(TsqError::new(
                    "VALIDATION_ERROR",
                    "stdin content must not be empty",
                    1,
                ));
            }
            Ok(content)
        }
        Err(RecvTimeoutError::Timeout) => Err(TsqError::new(
            "VALIDATION_ERROR",
            "stdin read timed out after 30 seconds",
            1,
        )),
        Err(RecvTimeoutError::Disconnected) => {
            Err(TsqError::new("IO_ERROR", "failed reading stdin", 2))
        }
    }
}
