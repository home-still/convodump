use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use hs_common::global_args::GlobalArgs;

const EXAMPLES: &str = "\
Examples:
  convodump media run talk.m4a                    Print a diarized, time-coded transcript
  convodump media run a.mp4 --out-dir out         Write out/a.convodump.json
  convodump media run talk.m4a --output json      One compact JSON document on stdout
  convodump server run                            Serve the transcription API
  convodump server status                         Check the configured server
";

const EXIT_CODES: &str = "
Exit codes:
  0  success
  1  general error (unreadable file, no audio stream, engine failure)
  2  usage or configuration error (missing token, bad flag)
  3  server unreachable
  4  partial success (some files failed)";

#[derive(Parser, Debug)]
#[command(
    name = "convodump",
    version,
    about = "Media file to JSON: metadata, diarized speaker segments, time codes and word timestamps",
    help_template = "{about-with-newline}\n{usage-heading} {usage}\n\n{before-help}{all-args}{after-help}",
    before_help = EXAMPLES,
    after_help = EXIT_CODES,
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Transcribe media files
    #[command(subcommand)]
    Media(MediaCommand),
    /// Run or inspect the transcription server
    #[command(subcommand)]
    Server(ServerCommand),
}

#[derive(Subcommand, Debug)]
pub enum MediaCommand {
    /// Upload media files to the configured server and print or save the result
    Run(MediaRunArgs),
}

#[derive(Subcommand, Debug)]
pub enum ServerCommand {
    /// Serve the transcription API (needs ffmpeg, ffprobe and nemo-speech)
    Run(ServerRunArgs),
    /// Show the configured server's health
    Status(ServerStatusArgs),
}

#[derive(Args, Debug)]
pub struct MediaRunArgs {
    /// Media files (audio or video)
    #[arg(required = true, value_name = "FILE")]
    pub files: Vec<PathBuf>,

    /// Language code passed to the engine (default: engine.language, "auto")
    #[arg(long, value_name = "CODE")]
    pub language: Option<String>,

    /// Write <DIR>/<file stem>.convodump.json instead of printing the result
    #[arg(long, value_name = "DIR")]
    pub out_dir: Option<PathBuf>,

    /// Overwrite existing output files
    #[arg(long)]
    pub force: bool,

    /// Server base URL (overrides client.server_url)
    #[arg(long, value_name = "URL")]
    pub server_url: Option<String>,
}

#[derive(Args, Debug)]
pub struct ServerRunArgs {
    /// Listen address (overrides server.bind)
    #[arg(long, value_name = "ADDR")]
    pub bind: Option<String>,
}

#[derive(Args, Debug)]
pub struct ServerStatusArgs {
    /// Server base URL (overrides client.server_url)
    #[arg(long, value_name = "URL")]
    pub server_url: Option<String>,
}
