use std::path::{Path, PathBuf};
use std::sync::Arc;

use convodump_core::config::{self, CliOverrides};
use hs_common::exit_codes::{GENERAL_ERROR, PARTIAL_SUCCESS, SUCCESS};
use hs_common::global_args::{GlobalArgs, OutputFormat};
use hs_common::reporter::Reporter;

use crate::cli::MediaRunArgs;
use crate::client;
use crate::exit::ExitError;
use crate::output::render_text;

const OUTPUT_SUFFIX: &str = ".convodump.json";

fn output_path(dir: &Path, input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    dir.join(format!("{stem}{OUTPUT_SUFFIX}"))
}

pub async fn run(args: MediaRunArgs, global: &GlobalArgs, reporter: Arc<dyn Reporter>) -> u8 {
    let cfg = match config::load(CliOverrides {
        server_url: args.server_url.clone(),
        bind: None,
    }) {
        Ok(cfg) => cfg,
        Err(e) => {
            ExitError::usage(e.to_string()).report(&reporter);
            return hs_common::exit_codes::USAGE_ERROR;
        }
    };
    if cfg.client.token.is_empty() {
        ExitError::usage("client.token is empty")
            .fix("set client.token in ~/.home-still/convodump/config.yaml")
            .report(&reporter);
        return hs_common::exit_codes::USAGE_ERROR;
    }
    let json_mode = matches!(global.output, OutputFormat::Json | OutputFormat::Ndjson);

    if let Some(dir) = &args.out_dir {
        if let Err(e) = std::fs::create_dir_all(dir) {
            ExitError::general(format!("cannot create {}", dir.display()))
                .why(e.to_string())
                .report(&reporter);
            return GENERAL_ERROR;
        }
    }

    let mut succeeded = 0usize;
    let mut first_failure: Option<u8> = None;
    for path in &args.files {
        match run_one(&args, &cfg.client, path, json_mode, &reporter).await {
            Ok(()) => succeeded += 1,
            Err(err) => {
                err.report(&reporter);
                first_failure.get_or_insert(err.code);
            }
        }
    }
    match (succeeded, first_failure) {
        (_, None) => SUCCESS,
        (0, Some(code)) => code,
        (_, Some(_)) => PARTIAL_SUCCESS,
    }
}

async fn run_one(
    args: &MediaRunArgs,
    client_cfg: &convodump_core::config::ClientConfig,
    path: &Path,
    json_mode: bool,
    reporter: &Arc<dyn Reporter>,
) -> Result<(), ExitError> {
    let meta = std::fs::metadata(path).map_err(|e| {
        ExitError::general(format!("cannot read {}", path.display())).why(e.to_string())
    })?;
    if !meta.is_file() {
        return Err(ExitError::general(format!(
            "{} is not a regular file",
            path.display()
        )));
    }
    let target = args.out_dir.as_deref().map(|d| output_path(d, path));
    if let Some(target) = &target {
        if target.exists() && !args.force {
            return Err(
                ExitError::general(format!("{} already exists", target.display()))
                    .fix("pass --force to overwrite it"),
            );
        }
    }

    let doc = client::transcribe(client_cfg, path, args.language.as_deref(), reporter).await?;

    match target {
        Some(target) => {
            let pretty = serde_json::to_string_pretty(&doc).map_err(|e| {
                ExitError::general("cannot serialize the result").why(e.to_string())
            })?;
            std::fs::write(&target, pretty + "\n").map_err(|e| {
                ExitError::general(format!("cannot write {}", target.display())).why(e.to_string())
            })?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({
                        "type": "result",
                        "input": doc.source.file_name,
                        "output": target.display().to_string(),
                    })
                );
            } else {
                println!("{}", target.display());
            }
        }
        None if json_mode => {
            let line = serde_json::to_string(&doc).map_err(|e| {
                ExitError::general("cannot serialize the result").why(e.to_string())
            })?;
            println!("{line}");
        }
        None => print!("{}", render_text(&doc)),
    }
    Ok(())
}
