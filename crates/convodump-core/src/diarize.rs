//! `nemo-speech diarize --format rttm` -> speaker turns.
//!
//! RTTM line: `SPEAKER <uri> <chan> <start> <dur> <ortho> <conf> <label> <na> <na>`.

use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

use crate::config::EngineConfig;
use crate::error::{stderr_tail, ConvodumpError, Result};

/// One speaker turn, in seconds from the start of the audio.
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub start_s: f64,
    pub end_s: f64,
    pub speaker: String,
}

/// Runs `diarize` and returns the turns it found. An empty recording yields an empty vector.
pub async fn run_diarize(cfg: &EngineConfig, wav: &Path, out_rttm: &Path) -> Result<Vec<Turn>> {
    let mut cmd = match cfg.wrapper.split_first() {
        Some((program, rest)) => {
            let mut c = Command::new(program);
            c.args(rest).arg(&cfg.nemo_speech_bin);
            c
        }
        None => Command::new(&cfg.nemo_speech_bin),
    };
    cmd.arg("diarize")
        .arg(wav)
        .args(["--format", "rttm", "--recording-id", "job", "--output"])
        .arg(out_rttm);
    if !cfg.diarization_preset.is_empty() {
        cmd.args(["--preset", &cfg.diarization_preset]);
    }
    let out = cmd
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| {
            ConvodumpError::Io(std::io::Error::new(
                e.kind(),
                format!("cannot run {:?}: {e}", cmd.as_std().get_program()),
            ))
        })?;
    if !out.status.success() {
        return Err(ConvodumpError::Engine {
            stderr_tail: stderr_tail(&out.stderr),
        });
    }
    let text = tokio::fs::read_to_string(out_rttm).await?;
    Ok(parse_rttm(&text))
}

/// Parses RTTM speaker turns, sorted by start. Lines with a non-positive duration are skipped.
pub fn parse_rttm(text: &str) -> Vec<Turn> {
    let mut turns: Vec<Turn> = text
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            if fields.next() != Some("SPEAKER") {
                return None;
            }
            let _uri = fields.next()?;
            let _channel = fields.next()?;
            let start_s: f64 = fields.next()?.parse().ok()?;
            let dur_s: f64 = fields.next()?.parse().ok()?;
            let _ortho = fields.next()?;
            let _conf = fields.next()?;
            let speaker = fields.next()?.to_owned();
            if !(start_s.is_finite() && dur_s.is_finite()) || dur_s <= 0.0 || start_s < 0.0 {
                return None;
            }
            Some(Turn {
                start_s,
                end_s: start_s + dur_s,
                speaker,
            })
        })
        .collect();
    turns.sort_by(|a, b| {
        a.start_s
            .total_cmp(&b.start_s)
            .then_with(|| a.end_s.total_cmp(&b.end_s))
    });
    turns
}
