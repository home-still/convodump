//! The transcription HTTP server.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use convodump_core::config::ConvodumpConfig;
use convodump_core::models::EngineInfo;
use tokio::sync::Semaphore;

pub mod auth;
pub mod routes;

const BYTES_PER_MB: u64 = 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<ConvodumpConfig>,
    /// Reported by `/v1/health`; `language_requested` carries the configured language.
    pub engine: Arc<EngineInfo>,
    pub work_dir: PathBuf,
    /// One permit: one job at a time.
    pub semaphore: Arc<Semaphore>,
}

impl AppState {
    pub fn new(cfg: ConvodumpConfig, engine: EngineInfo, work_dir: PathBuf) -> Self {
        Self {
            cfg: Arc::new(cfg),
            engine: Arc::new(engine),
            work_dir,
            semaphore: Arc::new(Semaphore::new(1)),
        }
    }

    pub fn busy(&self) -> bool {
        self.semaphore.available_permits() == 0
    }
}

pub fn router(state: AppState) -> Router {
    let limit = state.cfg.server.max_upload_mb.saturating_mul(BYTES_PER_MB) as usize;
    Router::new()
        .route("/v1/health", get(routes::health))
        .route(
            "/v1/media",
            post(routes::post_media).layer(DefaultBodyLimit::max(limit)),
        )
        .with_state(state)
}
