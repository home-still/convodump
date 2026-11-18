//! nemo-speech 0.3.0 CLI JSON (`--format json`) parsing and validation.
//!
//! Shape: `{file,text,confidence,duration,languages[],words[{word,start,end,confidence,speaker?}]}`
//! where `speaker` is 1-based.

use serde::Deserialize;

use crate::error::{ConvodumpError, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct EngineWord {
    pub word: String,
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub speaker: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EngineTranscript {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub words: Vec<EngineWord>,
}

pub fn parse_nemo_json(json: &str) -> Result<EngineTranscript> {
    let parsed: EngineTranscript = serde_json::from_str(json)
        .map_err(|e| ConvodumpError::EngineOutput(format!("not valid nemo-speech JSON: {e}")))?;

    for (i, w) in parsed.words.iter().enumerate() {
        let ok = w.start.is_finite() && w.end.is_finite() && w.start >= 0.0 && w.end >= w.start;
        if !ok {
            return Err(ConvodumpError::EngineOutput(format!(
                "word {i} has invalid timing (start {}, end {})",
                w.start, w.end
            )));
        }
    }
    if !parsed.text.trim().is_empty() && parsed.words.is_empty() {
        return Err(ConvodumpError::EngineOutput(
            "transcript text without word timings (alignment failed)".into(),
        ));
    }
    Ok(parsed)
}
