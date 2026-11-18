//! HTTP client for the ConvoDump server.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use convodump_core::config::ClientConfig;
use convodump_core::models::ConvoDump;
use futures_util::StreamExt;
use hs_common::reporter::{Reporter, StageHandle};
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use tokio_util::io::ReaderStream;

use crate::exit::ExitError;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const STATUS_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    code: String,
    message: String,
    cause: Option<String>,
    fix: Option<String>,
}

fn unreachable_error(url: &str, detail: impl std::fmt::Display) -> ExitError {
    ExitError::network(format!("cannot reach convodump server at {url}"))
        .why(detail.to_string())
        .fix("start it (convodump server run) or point client.server_url at the other server")
}

async fn server_error(url: &str, resp: reqwest::Response) -> ExitError {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    match serde_json::from_str::<ErrorEnvelope>(&text) {
        Ok(env) => {
            let code = if status == reqwest::StatusCode::UNAUTHORIZED {
                hs_common::exit_codes::USAGE_ERROR
            } else {
                hs_common::exit_codes::GENERAL_ERROR
            };
            let mut err =
                ExitError::new(code, format!("{} ({})", env.error.message, env.error.code));
            err.why = env.error.cause;
            err.fix = env.error.fix;
            err
        }
        Err(_) => ExitError::general(format!("server at {url} answered HTTP {status}"))
            .why("the response was not a convodump error body; is this a convodump server?"),
    }
}

/// `GET /v1/health`, returned as the server's JSON.
pub async fn health(server_url: &str) -> Result<serde_json::Value, ExitError> {
    let url = format!("{}/v1/health", server_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(STATUS_TIMEOUT)
        .build()
        .map_err(|e| ExitError::general(format!("cannot build the HTTP client: {e}")))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| unreachable_error(server_url, e))?;
    if !resp.status().is_success() {
        return Err(server_error(server_url, resp).await);
    }
    resp.json().await.map_err(|e| {
        ExitError::general("the server's health answer is not JSON").why(e.to_string())
    })
}

/// Upload one file and wait for its transcript.
pub async fn transcribe(
    cfg: &ClientConfig,
    path: &Path,
    language: Option<&str>,
    reporter: &Arc<dyn Reporter>,
) -> Result<ConvoDump, ExitError> {
    let server_url = cfg.server_url.trim_end_matches('/');
    let url = format!("{server_url}/v1/media");
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "input".into());

    let file = tokio::fs::File::open(path).await.map_err(|e| {
        ExitError::general(format!("cannot open {}", path.display())).why(e.to_string())
    })?;
    let len = file
        .metadata()
        .await
        .map_err(|e| {
            ExitError::general(format!("cannot stat {}", path.display())).why(e.to_string())
        })?
        .len();

    let upload: Arc<dyn StageHandle> = Arc::from(reporter.begin_stage("Uploading", Some(len)));
    let transcribing: Arc<OnceLock<Box<dyn StageHandle>>> = Arc::new(OnceLock::new());
    let sent = Arc::new(AtomicU64::new(0));
    let body_stream = {
        let upload = upload.clone();
        let transcribing = transcribing.clone();
        let reporter = reporter.clone();
        let sent = sent.clone();
        ReaderStream::new(file).map(move |chunk| {
            if let Ok(bytes) = &chunk {
                let n = bytes.len() as u64;
                upload.inc(n);
                if sent.fetch_add(n, Ordering::Relaxed) + n >= len {
                    upload.finish_and_clear();
                    transcribing.get_or_init(|| reporter.begin_stage("Transcribing", None));
                }
            }
            chunk
        })
    };

    let part = Part::stream_with_length(reqwest::Body::wrap_stream(body_stream), len)
        .file_name(file_name)
        .mime_str("application/octet-stream")
        .map_err(|e| ExitError::general(e.to_string()))?;
    let mut form = Form::new();
    if let Some(lang) = language {
        form = form.text("language", lang.to_owned());
    }
    let form = form.part("file", part);

    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(Duration::from_secs(cfg.timeout_secs))
        .build()
        .map_err(|e| ExitError::general(format!("cannot build the HTTP client: {e}")))?;
    let result = client
        .post(&url)
        .bearer_auth(&cfg.token)
        .multipart(form)
        .send()
        .await;

    upload.finish_and_clear();
    let finish_transcribing = |ok: bool| {
        if let Some(stage) = transcribing.get() {
            if ok {
                stage.finish_and_clear();
            } else {
                stage.finish_failed("failed");
            }
        }
    };

    let resp = match result {
        Ok(resp) => resp,
        Err(e) => {
            finish_transcribing(false);
            return Err(unreachable_error(server_url, e));
        }
    };
    if !resp.status().is_success() {
        finish_transcribing(false);
        return Err(server_error(server_url, resp).await);
    }
    let doc = resp.json::<ConvoDump>().await.map_err(|e| {
        finish_transcribing(false);
        ExitError::general("the server's answer is not a ConvoDump document").why(e.to_string())
    })?;
    finish_transcribing(true);
    Ok(doc)
}
