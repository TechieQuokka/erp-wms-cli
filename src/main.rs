//! `wms` — official CLI for the 3PL WMS headless API.

mod cli;
mod client;
mod commands;
mod config;
mod context;
mod error;
mod output;
mod util;

use clap::Parser;

use crate::cli::Cli;
use crate::context::RuntimeContext;
use crate::error::CliError;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let cli = Cli::parse();
    let code = match run(cli).await {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            e.exit_code()
        }
    };
    std::process::exit(code);
}

async fn run(cli: Cli) -> Result<(), CliError> {
    let ctx = RuntimeContext::resolve(&cli.global)?;
    commands::dispatch(cli.command, ctx).await
}
