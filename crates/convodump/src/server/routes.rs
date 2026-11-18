use std::path::Path;

use axum::extract::{FromRequest, Multipart, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use convodump_core::error::ConvodumpError;
use convodump_core::models::{ConvoDump, Source};
use convodump_core::pipeline::run_job;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::auth::bearer_matches;
use super::AppState;

const MAX_LANGUAGE_CHARS: usize = 16;
const FALLBACK_FILE_NAME: &str = "input";

pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    cause: Option<String>,
    fix: Option<String>,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            cause: None,
            fix: None,
        }
    }

    fn cause(mut self, cause: impl Into<String>) -> Self {
        self.cause = Some(cause.into());
        self
    }

    fn fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }

    fn internal(err: impl std::fmt::Display) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "internal server error",
        )
        .cause(err.to_string())
    }
}

impl From<ConvodumpError> for ApiError {
    fn from(err: ConvodumpError) -> Self {
        match err {
            ConvodumpError::NoAudioStream => Self::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "no_audio_stream",
                "the media has no audio stream",
            )
            .fix("send a file that contains an audio track"),
            ConvodumpError::UnreadableMedia { stderr_tail } => Self::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "unreadable_media",
                "the file is not readable media",
            )
            .cause(stderr_tail)
            .fix("check that the file is a complete audio or video file"),
            ConvodumpError::Decode { stderr_tail } => Self::new(
                StatusCode::BAD_GATEWAY,
                "engine_failed",
                "decoding the audio failed",
            )
            .cause(stderr_tail),
            ConvodumpError::Engine { stderr_tail } => Self::new(
                StatusCode::BAD_GATEWAY,
                "engine_failed",
                "the transcription engine failed",
            )
            .cause(stderr_tail)
            .fix("check the server log and that both models are pulled"),
            ConvodumpError::EngineOutput(msg) => Self::new(
                StatusCode::BAD_GATEWAY,
                "engine_failed",
                "the transcription engine produced invalid output",
            )
            .cause(msg),
            other @ (ConvodumpError::Config(_) | ConvodumpError::Io(_)) => Self::internal(other),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({"error": {
            "code": self.code,
            "message": self.message,
            "cause": self.cause,
            "fix": self.fix,
        }});
        (self.status, Json(body)).into_response()
    }
}

fn unauthorized() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "missing or wrong bearer token",
    )
    .fix("set client.token to the server's server.token")
}

fn multipart_error(err: axum::extract::multipart::MultipartError, max_mb: u64) -> ApiError {
    if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too_large",
            "the upload exceeds the size limit",
        )
        .fix(format!(
            "send a file under {max_mb} MB or raise server.max_upload_mb"
        ))
    } else {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "malformed multipart body",
        )
        .cause(err.body_text())
    }
}

fn valid_language(lang: &str) -> bool {
    !lang.is_empty()
        && lang.len() <= MAX_LANGUAGE_CHARS
        && !lang.starts_with('-')
        && lang
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "convodump_version": env!("CARGO_PKG_VERSION"),
        "engine": {
            "name": state.engine.name,
            "version": state.engine.version,
            "asr_model": state.engine.asr_model,
            "diarization_model": state.engine.diarization_model,
            "diarization_preset": state.engine.diarization_preset,
            "recognition_mode": state.engine.recognition_mode,
            "language": state.engine.language_requested,
        },
        "busy": state.busy(),
    }))
}

pub async fn post_media(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
) -> Result<Json<ConvoDump>, ApiError> {
    if !bearer_matches(&headers, &state.cfg.server.token) {
        return Err(unauthorized());
    }
    let max_mb = state.cfg.server.max_upload_mb;
    let mut multipart = Multipart::from_request(request, &state)
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "bad_request",
                "expected a multipart body",
            )
            .cause(e.body_text())
        })?;

    // One job at a time; later requests wait here without reading their body.
    let _permit = state
        .semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(ApiError::internal)?;

    // Deleted when this handler ends, on success, failure or client disconnect.
    let job = tempfile::Builder::new()
        .prefix("job-")
        .tempdir_in(&state.work_dir)
        .map_err(ApiError::internal)?;
    let job_id = job
        .path()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut language: Option<String> = None;
    let mut uploaded: Option<(String, u64, String)> = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| multipart_error(e, max_mb))?
    {
        let name = field.name().map(str::to_owned);
        match name.as_deref() {
            Some("language") => {
                let text = field.text().await.map_err(|e| multipart_error(e, max_mb))?;
                let text = text.trim().to_owned();
                if !valid_language(&text) {
                    return Err(ApiError::new(
                        StatusCode::BAD_REQUEST,
                        "bad_request",
                        "invalid language code",
                    ));
                }
                language = Some(text);
            }
            Some("file") => {
                let file_name = field
                    .file_name()
                    .and_then(|n| Path::new(n).file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| FALLBACK_FILE_NAME.to_owned());
                let mut out = tokio::fs::File::create(job.path().join("input"))
                    .await
                    .map_err(ApiError::internal)?;
                let mut hasher = Sha256::new();
                let mut size: u64 = 0;
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|e| multipart_error(e, max_mb))?
                {
                    hasher.update(&chunk);
                    size += chunk.len() as u64;
                    out.write_all(&chunk).await.map_err(ApiError::internal)?;
                }
                out.flush().await.map_err(ApiError::internal)?;
                uploaded = Some((file_name, size, hex(&hasher.finalize())));
            }
            _ => {}
        }
    }
    let (file_name, size_bytes, sha256) = uploaded.ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "missing_file",
            "no `file` field in the multipart body",
        )
        .fix("send the media as the multipart field `file`")
    })?;

    let language = language.unwrap_or_else(|| state.engine.language_requested.clone());
    let source = Source {
        file_name,
        size_bytes,
        sha256,
    };
    // Logs carry the job id, size, timings and outcome only: never the file name or any text.
    match run_job(
        &state.cfg.engine,
        &state.engine.version,
        job.path(),
        source,
        &language,
    )
    .await
    {
        Ok(doc) => {
            tracing::info!(
                job = %job_id, size_bytes, duration_s = doc.transcript.duration_s,
                probe_ms = doc.timing.probe_ms, decode_ms = doc.timing.decode_ms,
                engine_ms = doc.timing.engine_ms, total_ms = doc.timing.total_ms,
                outcome = "ok", "job finished"
            );
            Ok(Json(doc))
        }
        Err(err) => {
            let api: ApiError = err.into();
            tracing::warn!(job = %job_id, size_bytes, outcome = api.code, "job failed");
            Err(api)
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
