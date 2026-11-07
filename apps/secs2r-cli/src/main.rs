#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! `secs2r-cli`: the SECS 2R service without a screen.
//!
//! Commands call the application service and write JSON Lines to standard output.

mod args;
mod commands;
mod runner;

use args::Cli;
use clap::Parser;
use commands::run;
use runner::{PrintEvents, Runner, deps};
use secs_api::EngineBuilder;
use std::process::ExitCode;
use std::sync::Arc;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let engine = EngineBuilder::from_deps(deps(cli.deterministic_uuids, cli.compute))
        .observer(Arc::new(PrintEvents::new(cli.quiet)))
        .build();
    let mut runner = Runner { engine, ok: true };
    run(cli.action, &mut runner);
    if runner.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
