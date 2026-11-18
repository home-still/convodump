//! The ConvoDump output document (schema version 1).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const ENGINE_NAME: &str = "nemo-speech";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConvoDump {
    pub schema_version: u32,
    pub convodump_version: String,
    pub source: Source,
    pub media: Media,
    pub engine: EngineInfo,
    pub transcript: Transcript,
    pub timing: Timing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Source {
    /// Basename only; never a path.
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Media {
    pub format_name: String,
    pub format_long_name: Option<String>,
    pub duration_s: Option<f64>,
    pub size_bytes: Option<u64>,
    pub bit_rate: Option<u64>,
    pub tags: BTreeMap<String, String>,
    pub streams: Vec<serde_json::Value>,
    pub chapters: Vec<serde_json::Value>,
    pub transcribed_stream_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EngineInfo {
    pub name: String,
    pub version: String,
    pub asr_model: String,
    pub diarization_model: String,
    pub language_requested: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transcript {
    pub text: String,
    pub languages: Vec<String>,
    pub duration_s: f64,
    pub confidence: Option<f64>,
    pub speakers: Vec<Speaker>,
    pub segments: Vec<Segment>,
    pub words: Vec<Word>,
    pub unattributed_word_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Speaker {
    /// Anonymous id, `S1`, `S2`, ...
    pub id: String,
    pub word_count: u32,
    pub segment_count: u32,
    pub talk_time_s: f64,
    pub first_start_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Segment {
    pub index: u32,
    pub speaker: Option<String>,
    pub start_s: f64,
    pub end_s: f64,
    pub start_tc: String,
    pub end_tc: String,
    pub text: String,
    pub first_word: u32,
    pub last_word: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Word {
    pub index: u32,
    pub text: String,
    pub start_s: f64,
    pub end_s: f64,
    pub confidence: Option<f64>,
    pub speaker: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Timing {
    pub probe_ms: u64,
    pub decode_ms: u64,
    pub engine_ms: u64,
    pub total_ms: u64,
}
