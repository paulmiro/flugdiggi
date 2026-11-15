mod cli;
mod commands;
mod tables;

use std::io::Write;

use clap::Parser;
use serde_json::{Value, json};
use tracing_subscriber::EnvFilter;

use crate::cli::Cli;
use crate::commands::Failure;

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("flugdiggi_core=info")))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let reply = match commands::run(cli.command) {
        Ok(Value::Object(mut fields)) => {
            fields.insert("ok".into(), true.into());
            Value::Object(fields)
        }
        Ok(other) => json!({ "ok": true, "value": other }),
        Err(Failure { kind, message }) => json!({ "ok": false, "error": { "kind": kind, "message": message } }),
    };
    // Failing to write the reply leaves the caller without an answer, so that is a crash.
    match writeln!(std::io::stdout().lock(), "{reply}") {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => std::process::ExitCode::FAILURE,
    }
}
