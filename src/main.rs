mod cli;
mod commands;
mod daemon;
mod error;
mod mcp;
mod output;
mod server_cmd;
mod state;
mod tools;
mod transport;

use clap::Parser;
use cli::Cli;
use error::CliError;
use output::{Format, OutputOpts};
use std::io::Write;
use std::time::Duration;
use transport::{Ctx, CtxOptions};

#[tokio::main]
async fn main() {
    let code = run().await;
    std::process::exit(code);
}

async fn run() -> i32 {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            use clap::error::ErrorKind;
            if matches!(
                err.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                let _ = err.print();
                return 0;
            }
            let usage = CliError::usage(err.to_string().trim_end().to_owned());
            report(&usage);
            return usage.exit_code();
        }
    };

    let global = cli.global.clone();
    let server_cmd = match server_cmd::resolve(global.server_cmd.as_deref()) {
        Ok(cmd) => cmd,
        Err(e) => {
            report(&e);
            return e.exit_code();
        }
    };
    let format = if global.text {
        Some(Format::Text)
    } else {
        global.format
    };
    let ctx = Ctx::new(CtxOptions {
        mode: global.transport,
        state_dir: state::state_dir(global.state_dir.as_deref()),
        server_cmd,
        timeout: Duration::from_secs(global.timeout.max(1)),
        format,
        compact: global.compact,
        raw: global.raw,
        quiet: global.quiet,
    });

    let result = commands::dispatch(&ctx, cli.command).await;
    let code = match result {
        Ok(outcome) => {
            let opts = OutputOpts {
                format: ctx.format.unwrap_or(Format::Json),
                compact: ctx.compact,
            };
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            if let Err(e) = outcome.output.write_to(&mut lock, opts)
                && e.kind() != std::io::ErrorKind::BrokenPipe
            {
                eprintln!("bcp: failed to write output: {e}");
            }
            let _ = lock.flush();
            match outcome.error {
                Some(err) => {
                    report(&err);
                    err.exit_code()
                }
                None => 0,
            }
        }
        Err(err) => {
            report(&err);
            err.exit_code()
        }
    };
    ctx.finish().await;
    code
}

fn report(err: &CliError) {
    eprintln!("{}", err.to_json());
}
