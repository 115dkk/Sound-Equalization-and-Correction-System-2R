#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! `secs2r-cli`: the SECS 2R service without a screen.
//!
//! Every subcommand is one service request; the reply envelope is printed as
//! JSON on stdout and the exit code is 0 for `ok: true`, 1 otherwise. Design
//! and export commands arrive with the engine (M2) and the service (M4); CI's
//! integrity job and the performance audits run them.

use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use secs_service::{Deps, NoEvents, Service};
use secs_types::ipc::{Envelope, Request};

#[derive(Debug, Parser)]
#[command(name = "secs2r-cli", version, about = "SECS 2R without a screen")]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, Subcommand)]
enum Action {
    /// Print the version, the upstream SECS 2 release and the platform.
    Bootstrap,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let service = Service::new(Deps::default());
    let envelope = match cli.action {
        Action::Bootstrap => service.call(Request::Bootstrap, Arc::new(NoEvents)),
    };
    match serde_json::to_string_pretty(&envelope) {
        Ok(text) => println!("{text}"),
        Err(error) => {
            eprintln!("secs2r-cli: could not print the reply: {error}");
            return ExitCode::FAILURE;
        }
    }
    match envelope {
        Envelope::Ok(_) => ExitCode::SUCCESS,
        Envelope::Err(_) => ExitCode::FAILURE,
    }
}
