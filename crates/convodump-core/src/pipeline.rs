//! One job: ffprobe -> ffmpeg decode -> diarize -> offline chunked recognition -> [`ConvoDump`].

use std::path::Path;
use std::process::Stdio;
use std::time::Instant;

use tokio::process::Command;

use crate::chunk::plan_cuts;
use crate::config::EngineConfig;
use crate::diarize::run_diarize;
use crate::engine::{parse_nemo_json, EngineTranscript, EngineWord};
use crate::error::{stderr_tail, ConvodumpError, Result};
use crate::models::{ConvoDump, EngineInfo, Source, Timing, ENGINE_NAME, SCHEMA_VERSION};
use crate::probe::parse_ffprobe;
use crate::segments::build_transcript_with_turns;

const AUDIO_CHANNELS: &str = "1";
const AUDIO_SAMPLE_RATE_HZ: &str = "16000";

struct Output {
    status_ok: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

async fn run(mut cmd: Command) -> Result<Output> {
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
    Ok(Output {
        status_ok: out.status.success(),
        stdout: out.stdout,
        stderr: out.stderr,
    })
}

/// `job_dir` contains the uploaded file as `input`; the decoded audio, the RTTM and one JSON per
/// chunk are written there.
pub async fn run_job(
    cfg: &EngineConfig,
    engine_version: &str,
    job_dir: &Path,
    source: Source,
    language: &str,
) -> Result<ConvoDump> {
    let total = Instant::now();
    let input = job_dir.join("input");
    let wav = job_dir.join("audio.wav");

    // 1. probe
    let t = Instant::now();
    let mut cmd = Command::new(&cfg.ffprobe_bin);
    cmd.args([
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
        "-show_chapters",
    ])
    .arg(&input);
    let out = run(cmd).await?;
    if !out.status_ok {
        return Err(ConvodumpError::UnreadableMedia {
            stderr_tail: stderr_tail(&out.stderr),
        });
    }
    let media = parse_ffprobe(&String::from_utf8_lossy(&out.stdout))?;
    let probe_ms = t.elapsed().as_millis() as u64;

    // 2. decode
    let mut decode_ms = 0u64;
    let t = Instant::now();
    let mut cmd = Command::new(&cfg.ffmpeg_bin);
    cmd.args(["-nostdin", "-v", "error", "-i"])
        .arg(&input)
        .arg("-map")
        .arg(format!("0:{}", media.transcribed_stream_index))
        .args([
            "-ac",
            AUDIO_CHANNELS,
            "-ar",
            AUDIO_SAMPLE_RATE_HZ,
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&wav);
    let out = run(cmd).await?;
    if !out.status_ok {
        return Err(ConvodumpError::Decode {
            stderr_tail: stderr_tail(&out.stderr),
        });
    }
    decode_ms += t.elapsed().as_millis() as u64;

    // 3. diarize: the turns become both the chunk cuts and the transcript's segments
    let t = Instant::now();
    let turns = run_diarize(cfg, &wav, &job_dir.join("diar.rttm")).await?;
    let diarize_ms = t.elapsed().as_millis() as u64;

    // 4. cut, then recognize each chunk offline (full attention, optionally behind a wrapper)
    let spans = match media.duration_s {
        Some(duration_s) => plan_cuts(&turns, duration_s, cfg.offline_chunk_max_s),
        // ffprobe reports a duration for every container tested; if it ever does not, transcribe
        // the whole file in one pass rather than failing the job.
        None => vec![(0.0, f64::INFINITY)],
    };
    let recognition_mode = if spans.len() > 1 {
        "offline-chunked"
    } else {
        "offline"
    };

    let t = Instant::now();
    let mut merged = MergedTranscript::default();
    for (index, (start_s, end_s)) in spans.iter().enumerate() {
        let chunk_wav = job_dir.join(format!("chunk-{index}.wav"));
        let engine_json = job_dir.join(format!("engine-{index}.json"));

        let t_split = Instant::now();
        let mut cmd = Command::new(&cfg.ffmpeg_bin);
        cmd.args(["-nostdin", "-v", "error", "-ss"])
            .arg(format!("{start_s:.3}"));
        if end_s.is_finite() {
            cmd.args(["-to"]).arg(format!("{end_s:.3}"));
        }
        cmd.arg("-i")
            .arg(&wav)
            .args(["-c:a", "pcm_s16le"])
            .arg(&chunk_wav);
        let out = run(cmd).await?;
        if !out.status_ok {
            return Err(ConvodumpError::Decode {
                stderr_tail: stderr_tail(&out.stderr),
            });
        }
        decode_ms += t_split.elapsed().as_millis() as u64;

        let mut cmd = match cfg.wrapper.split_first() {
            Some((program, rest)) => {
                let mut c = Command::new(program);
                c.args(rest).arg(&cfg.nemo_speech_bin);
                c
            }
            None => Command::new(&cfg.nemo_speech_bin),
        };
        cmd.arg("transcribe")
            .arg(&chunk_wav)
            .args(["--model", &cfg.asr_model])
            .args(["--language", language, "--format", "json", "--output"])
            .arg(&engine_json);
        let out = run(cmd).await?;
        if !out.status_ok {
            return Err(ConvodumpError::Engine {
                stderr_tail: format!("chunk {index}: {}", stderr_tail(&out.stderr)),
            });
        }
        let engine_text = tokio::fs::read_to_string(&engine_json).await?;
        merged.push(parse_nemo_json(&engine_text)?, *start_s);
    }
    let engine_ms = t.elapsed().as_millis() as u64;

    let transcript = build_transcript_with_turns(merged.finish(), &turns);

    Ok(ConvoDump {
        schema_version: SCHEMA_VERSION,
        convodump_version: env!("CARGO_PKG_VERSION").into(),
        source,
        media,
        engine: EngineInfo {
            name: ENGINE_NAME.into(),
            version: engine_version.into(),
            asr_model: cfg.asr_model.clone(),
            diarization_model: cfg.diarization_model.clone(),
            diarization_preset: cfg.diarization_preset.clone(),
            recognition_mode: recognition_mode.into(),
            language_requested: language.into(),
        },
        transcript,
        timing: Timing {
            probe_ms,
            decode_ms,
            diarize_ms,
            engine_ms,
            total_ms: total.elapsed().as_millis() as u64,
        },
    })
}

/// Concatenates per-chunk recognition output, shifting each chunk's word times onto the file timeline.
#[derive(Default)]
struct MergedTranscript {
    text: Vec<String>,
    confidence: Vec<f64>,
    languages: Vec<String>,
    duration_s: f64,
    words: Vec<EngineWord>,
}

impl MergedTranscript {
    fn push(&mut self, chunk: EngineTranscript, offset_s: f64) {
        if !chunk.text.trim().is_empty() {
            self.text.push(chunk.text);
        }
        if let Some(c) = chunk.confidence {
            self.confidence.push(c);
        }
        for lang in chunk.languages {
            if !self.languages.contains(&lang) {
                self.languages.push(lang);
            }
        }
        self.duration_s = self.duration_s.max(offset_s + chunk.duration);
        for mut w in chunk.words {
            w.start += offset_s;
            w.end += offset_s;
            self.words.push(w);
        }
    }

    fn finish(self) -> EngineTranscript {
        EngineTranscript {
            text: self.text.join(" "),
            confidence: match self.confidence.is_empty() {
                true => None,
                false => Some(self.confidence.iter().sum::<f64>() / self.confidence.len() as f64),
            },
            duration: self.duration_s,
            languages: self.languages,
            words: self.words,
        }
    }
}
