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

/// Scalar flags in their existing edit order. The clap definitions stay in args.
pub(crate) const SCALAR_FLAGS: &[(&str, &str)] = &[
    ("mimo", "mimo_on"),
    ("mimo-supports", "mimo_fir"),
    ("mimo-early", "mimo_early"),
    ("mimo-seat-guard", "mimo_seat_guard"),
    ("low-protect", "low_protect"),
    ("lf-align", "lf_align"),
    ("mode-decay", "mode_decay"),
    ("hf-phase", "hf_phase"),
    ("dip-limit", "dip_limit"),
    ("input-hp", "input_hp"),
];

fn main() -> ExitCode {
    // Hidden introspection for the integration test, before any engine or file opens.
    // Keep the clap definition and its public help byte-for-byte unchanged.
    if std::env::args_os()
        .skip(1)
        .eq(["project", "set", "--list-flags"].map(std::ffi::OsString::from))
    {
        println!(
            "{}",
            serde_json::to_string(SCALAR_FLAGS).expect("scalar flag table serializes")
        );
        return ExitCode::SUCCESS;
    }
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
