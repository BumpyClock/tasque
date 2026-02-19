use crate::types::{Envelope, EnvelopeErr, EnvelopeError, EnvelopeOk, SCHEMA_VERSION};
use serde_json::Value;

pub fn ok_envelope<T>(command: impl Into<String>, data: T) -> Envelope<T> {
    Envelope::Ok(EnvelopeOk {
        schema_version: SCHEMA_VERSION,
        command: command.into(),
        ok: true,
        data,
    })
}

pub fn err_envelope(
    command: impl Into<String>,
    code: impl Into<String>,
    message: impl Into<String>,
    details: Option<Value>,
) -> Envelope<Value> {
    Envelope::Err(EnvelopeErr {
        schema_version: SCHEMA_VERSION,
        command: command.into(),
        ok: false,
        error: EnvelopeError {
            code: code.into(),
            message: message.into(),
            details,
        },
    })
}
