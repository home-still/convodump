use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use convodump::cli::{Cli, Command, MediaCommand, ServerCommand};
use convodump::commands;
use hs_common::exit_codes::USAGE_ERROR;
use hs_common::global_args::OutputFormat;
use hs_common::mode::{detect, OutputMode};
use hs_common::pipe_reporter::PipeReporter;
use hs_common::reporter::{Reporter, SilentReporter};
use hs_common::tty_reporter::TtyReporter;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let _ = e.print();
            return ExitCode::from(if e.use_stderr() { USAGE_ERROR } else { 0 });
        }
    };

    let is_json = matches!(cli.global.output, OutputFormat::Json | OutputFormat::Ndjson);
    let reporter: Arc<dyn Reporter> = if cli.global.quiet {
        Arc::new(SilentReporter)
    } else {
        match detect(cli.global.color_str(), is_json) {
            OutputMode::Rich => Arc::new(TtyReporter::new(true)),
            OutputMode::Plain => Arc::new(TtyReporter::new(false)),
            OutputMode::Pipe => Arc::new(PipeReporter),
        }
    };

    let code = match cli.command {
        Command::Media(MediaCommand::Run(args)) => {
            commands::media::run(args, &cli.global, reporter).await
        }
        Command::Server(ServerCommand::Run(args)) => {
            commands::server::run(args, &cli.global, reporter).await
        }
        Command::Server(ServerCommand::Status(args)) => {
            commands::server::status(args, &cli.global, reporter).await
        }
    };
    ExitCode::from(code)
}
