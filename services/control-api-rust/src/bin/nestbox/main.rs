//! `nestbox` — the operator's command line for the API this repository serves.
//!
//! The CLI is a second client, not a second implementation: every command is
//! one HTTP call to the same routes the console uses, with the same session,
//! the same CSRF rule and the same error codes. That is what makes "what the
//! console can do" and "what the CLI can do" the same sentence.
//!
//! Configuration comes from the environment, so no credential has to sit in a
//! shell history:
//!
//!   NESTBOX_URL       default http://127.0.0.1:8080
//!   NESTBOX_EMAIL     the operator's login
//!   NESTBOX_PASSWORD  the operator's password
//!
//! Exit codes: 0 success, 1 a request failed, 2 a usage error.

use std::process::ExitCode;

use serde_json::Value;

mod api;
mod commands;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match commands::run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("nestbox: {}", error.message);
            ExitCode::from(error.code)
        }
    }
}

/// One failure, with the exit code it deserves.
pub struct Failure {
    pub message: String,
    pub code: u8,
}

impl Failure {
    pub fn usage(message: impl Into<String>) -> Self {
        Self { message: message.into(), code: 2 }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self { message: message.into(), code: 1 }
    }
}

/// Print a JSON document the way a terminal can read it.
pub fn print(value: &Value) {
    println!("{}", serde_json::to_string_pretty(value).unwrap_or_default());
}
