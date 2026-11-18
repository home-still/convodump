use std::process::Stdio;
use std::sync::Arc;

use convodump_core::config::{self, CliOverrides, ConvodumpConfig};
use convodump_core::models::{EngineInfo, ENGINE_NAME};
use hs_common::exit_codes::SUCCESS;
use hs_common::global_args::{GlobalArgs, OutputFormat};
use hs_common::reporter::Reporter;
use tokio::process::Command;

use crate::cli::{ServerRunArgs, ServerStatusArgs};
use crate::client;
use crate::exit::ExitError;
use crate::server::{router, AppState};

pub async fn status(
    args: ServerStatusArgs,
    global: &GlobalArgs,
    reporter: Arc<dyn Reporter>,
) -> u8 {
    let cfg = match config::load(CliOverrides {
        server_url: args.server_url,
        bind: None,
    }) {
        Ok(cfg) => cfg,
        Err(e) => {
            ExitError::usage(e.to_string()).report(&reporter);
            return hs_common::exit_codes::USAGE_ERROR;
        }
    };
    let url = cfg.client.server_url.trim_end_matches('/').to_owned();
    let health = match client::health(&url).await {
        Ok(h) => h,
        Err(e) => {
            e.report(&reporter);
            return e.code;
        }
    };
    if matches!(global.output, OutputFormat::Json | OutputFormat::Ndjson) {
        println!("{health}");
    } else {
        let text = |v: &serde_json::Value| v.as_str().unwrap_or("?").to_owned();
        println!("server {url} {}", text(&health["status"]));
        println!(
            "engine {} {}",
            text(&health["engine"]["name"]),
            text(&health["engine"]["version"])
        );
        println!("asr {}", text(&health["engine"]["asr_model"]));
        println!(
            "diarization {}",
            text(&health["engine"]["diarization_model"])
        );
        println!("busy {}", health["busy"].as_bool().unwrap_or(false));
    }
    SUCCESS
}

async fn tool_output(bin: &str, flag: &str) -> Result<String, ExitError> {
    let out = Command::new(bin)
        .arg(flag)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| {
            ExitError::usage(format!("cannot run {bin}"))
                .why(e.to_string())
                .fix(format!(
                    "install it, or set the absolute path in engine config ({bin})"
                ))
        })?;
    if !out.status.success() {
        return Err(
            ExitError::usage(format!("{bin} {flag} exited with {}", out.status))
                .fix("check that the binary works when run by hand"),
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Startup checks; each failure is exit 2 with what / why / fix.
async fn prepare(cfg: &ConvodumpConfig) -> Result<(EngineInfo, std::path::PathBuf), ExitError> {
    if cfg.server.token.is_empty() {
        return Err(ExitError::usage("server.token is empty")
            .why("the server refuses to run without a bearer token")
            .fix("set server.token in ~/.home-still/convodump/config.yaml"));
    }
    let work_dir = cfg
        .server
        .work_dir_path()
        .map_err(|e| ExitError::usage(e.to_string()))?;
    std::fs::create_dir_all(&work_dir).map_err(|e| {
        ExitError::usage(format!("cannot create work_dir {}", work_dir.display()))
            .why(e.to_string())
            .fix("set server.work_dir to a writable directory")
    })?;
    tool_output(&cfg.engine.ffprobe_bin, "-version").await?;
    tool_output(&cfg.engine.ffmpeg_bin, "-version").await?;
    let version = tool_output(&cfg.engine.nemo_speech_bin, "--version").await?;
    let info = EngineInfo {
        name: ENGINE_NAME.into(),
        version,
        asr_model: cfg.engine.asr_model.clone(),
        diarization_model: cfg.engine.diarization_model.clone(),
        language_requested: cfg.engine.language.clone(),
    };
    Ok((info, work_dir))
}

pub async fn run(args: ServerRunArgs, global: &GlobalArgs, reporter: Arc<dyn Reporter>) -> u8 {
    let cfg = match config::load(CliOverrides {
        server_url: None,
        bind: args.bind,
    }) {
        Ok(cfg) => cfg,
        Err(e) => {
            ExitError::usage(e.to_string()).report(&reporter);
            return hs_common::exit_codes::USAGE_ERROR;
        }
    };
    let (engine, work_dir) = match prepare(&cfg).await {
        Ok(v) => v,
        Err(e) => {
            e.report(&reporter);
            return e.code;
        }
    };

    let level = if global.verbose { "debug" } else { "info" };
    // Logs go to stderr and never contain file names or transcript text.
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::new(level))
        .try_init();

    let bind = cfg.server.bind.clone();
    let listener = match tokio::net::TcpListener::bind(&bind).await {
        Ok(l) => l,
        Err(e) => {
            ExitError::usage(format!("cannot listen on {bind}"))
                .why(e.to_string())
                .fix("set server.bind to a free address")
                .report(&reporter);
            return hs_common::exit_codes::USAGE_ERROR;
        }
    };
    tracing::info!(%bind, engine_version = %engine.version, "convodump server listening");
    let app = router(AppState::new(cfg, engine, work_dir));
    let served = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    if let Err(e) = served {
        ExitError::general("the server stopped with an error")
            .why(e.to_string())
            .report(&reporter);
        return hs_common::exit_codes::GENERAL_ERROR;
    }
    SUCCESS
}
