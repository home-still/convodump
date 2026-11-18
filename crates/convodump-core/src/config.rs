//! Layered configuration: compiled defaults < YAML file < `HOME_STILL_CONVODUMP_*` env < CLI overrides.
//!
//! The file is `<config_dir>/convodump/config.yaml`, where `config_dir` is
//! `$HOME_STILL_CONFIG_DIR` or `~/.home-still`. The global `~/.home-still/config.yaml`
//! is deliberately not read: it holds other tools' keys.

use std::path::{Path, PathBuf};

use figment::providers::{Env, Format, Serialized, Yaml};
use figment::Figment;
use serde::{Deserialize, Serialize};

use crate::error::{ConvodumpError, Result};

pub const ENV_PREFIX: &str = "HOME_STILL_CONVODUMP_";
pub const CONFIG_DIR_ENV: &str = "HOME_STILL_CONFIG_DIR";
pub const DEFAULT_CONFIG_DIR_NAME: &str = ".home-still";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct ConvodumpConfig {
    pub client: ClientConfig,
    pub server: ServerConfig,
    pub engine: EngineConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct ClientConfig {
    pub server_url: String,
    pub token: String,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct ServerConfig {
    pub bind: String,
    pub token: String,
    pub max_upload_mb: u64,
    pub work_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct EngineConfig {
    pub nemo_speech_bin: String,
    pub ffmpeg_bin: String,
    pub ffprobe_bin: String,
    pub asr_model: String,
    pub diarization_model: String,
    pub language: String,
    /// argv prefix placed before `nemo_speech_bin` (e.g. a GPU scheduler).
    pub wrapper: Vec<String>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_url: "http://127.0.0.1:7450".into(),
            token: String::new(),
            timeout_secs: 7200,
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:7450".into(),
            token: String::new(),
            max_upload_mb: 8192,
            work_dir: "~/.cache/home-still/convodump".into(),
        }
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            nemo_speech_bin: "nemo-speech".into(),
            ffmpeg_bin: "ffmpeg".into(),
            ffprobe_bin: "ffprobe".into(),
            asr_model: "nvidia/nemotron-3.5-asr-streaming-0.6b".into(),
            diarization_model: "nvidia/Nemotron-3-Diarization".into(),
            language: "auto".into(),
            wrapper: Vec::new(),
        }
    }
}

/// Values given on the command line; they win over every other layer.
#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
    pub server_url: Option<String>,
    pub bind: Option<String>,
}

pub fn config_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os(CONFIG_DIR_ENV) {
        return Ok(PathBuf::from(dir));
    }
    dirs::home_dir()
        .map(|h| h.join(DEFAULT_CONFIG_DIR_NAME))
        .ok_or_else(|| ConvodumpError::Config("cannot determine the home directory".into()))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("convodump").join("config.yaml"))
}

pub fn load(overrides: CliOverrides) -> Result<ConvodumpConfig> {
    load_from(&config_path()?, overrides)
}

pub fn load_from(file: &Path, overrides: CliOverrides) -> Result<ConvodumpConfig> {
    let mut cfg: ConvodumpConfig = Figment::new()
        .merge(Serialized::defaults(ConvodumpConfig::default()))
        .merge(Yaml::file(file))
        .merge(Env::prefixed(ENV_PREFIX).split("__"))
        .extract()
        .map_err(|e| ConvodumpError::Config(format!("{}: {e}", file.display())))?;
    if let Some(url) = overrides.server_url {
        cfg.client.server_url = url;
    }
    if let Some(bind) = overrides.bind {
        cfg.server.bind = bind;
    }
    Ok(cfg)
}

impl ServerConfig {
    /// `work_dir` with a leading `~/` expanded.
    pub fn work_dir_path(&self) -> Result<PathBuf> {
        match self.work_dir.strip_prefix("~/") {
            Some(rest) => dirs::home_dir().map(|h| h.join(rest)).ok_or_else(|| {
                ConvodumpError::Config("cannot determine the home directory".into())
            }),
            None => Ok(PathBuf::from(&self.work_dir)),
        }
    }
}
