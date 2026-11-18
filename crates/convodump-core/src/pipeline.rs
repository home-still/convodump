//! One job: ffprobe -> ffmpeg decode -> nemo-speech -> [`ConvoDump`].

use std::path::Path;
use std::process::Stdio;
use std::time::Instant;

use tokio::process::Command;

use crate::config::EngineConfig;
use crate::engine::parse_nemo_json;
use crate::error::{stderr_tail, ConvodumpError, Result};
use crate::models::{ConvoDump, EngineInfo, Source, Timing, ENGINE_NAME, SCHEMA_VERSION};
use crate::probe::parse_ffprobe;
use crate::segments::build_transcript;

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

/// `job_dir` contains the uploaded file as `input`; `audio.wav` and `engine.json` are written there.
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
    let engine_json = job_dir.join("engine.json");

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
    let decode_ms = t.elapsed().as_millis() as u64;

    // 3. engine (optionally behind a wrapper, e.g. a GPU scheduler)
    let t = Instant::now();
    let mut cmd = match cfg.wrapper.split_first() {
        Some((program, rest)) => {
            let mut c = Command::new(program);
            c.args(rest).arg(&cfg.nemo_speech_bin);
            c
        }
        None => Command::new(&cfg.nemo_speech_bin),
    };
    cmd.arg("transcribe")
        .arg(&wav)
        .args([
            "--model",
            &cfg.asr_model,
            "--diar-model",
            &cfg.diarization_model,
        ])
        .args(["--language", language, "--format", "json", "--output"])
        .arg(&engine_json);
    let out = run(cmd).await?;
    if !out.status_ok {
        return Err(ConvodumpError::Engine {
            stderr_tail: stderr_tail(&out.stderr),
        });
    }
    let engine_text = tokio::fs::read_to_string(&engine_json).await?;
    let transcript = build_transcript(parse_nemo_json(&engine_text)?);
    let engine_ms = t.elapsed().as_millis() as u64;

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
            language_requested: language.into(),
        },
        transcript,
        timing: Timing {
            probe_ms,
            decode_ms,
            engine_ms,
            total_ms: total.elapsed().as_millis() as u64,
        },
    })
}
