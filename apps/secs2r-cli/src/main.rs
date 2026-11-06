#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! `secs2r-cli`: the SECS 2R service without a screen.
//!
//! Commands call the application service and write JSON Lines to standard output.

mod apply;
mod args;
mod commands;
mod runner;

use args::Cli;
use clap::Parser;
use commands::run;
use runner::{Runner, deps};
use secs_service::Service;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut runner = Runner {
        service: Service::new(deps(cli.deterministic_uuids, cli.compute)),
        quiet: cli.quiet,
        ok: true,
    };
    run(cli.action, &mut runner);
    if runner.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
