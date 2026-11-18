//! In-process server tests: auth, health, and a full job with stub engine binaries.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use convodump::server::{router, AppState};
use convodump_core::config::ConvodumpConfig;
use convodump_core::models::{ConvoDump, EngineInfo};

const TOKEN: &str = "test-token";
const NEMO_FIXTURE: &str =
    include_str!("../../convodump-core/tests/fixtures/nemo_two_speakers.json");
const FFPROBE_FIXTURE: &str = include_str!("../../convodump-core/tests/fixtures/ffprobe_mp4.json");
const FFPROBE_NOAUDIO_FIXTURE: &str =
    include_str!("../../convodump-core/tests/fixtures/ffprobe_noaudio.json");
const FFPROBE_LONG_FIXTURE: &str =
    include_str!("../../convodump-core/tests/fixtures/ffprobe_long.json");
const RTTM_FIXTURE: &str =
    include_str!("../../convodump-core/tests/fixtures/diar_two_speakers.rttm");

fn engine_info() -> EngineInfo {
    EngineInfo {
        name: "nemo-speech".into(),
        version: "9.9.9-test".into(),
        asr_model: "asr-test".into(),
        diarization_model: "diar-test".into(),
        diarization_preset: "v3-offline".into(),
        recognition_mode: "offline-chunked".into(),
        language_requested: "auto".into(),
    }
}

async fn spawn(cfg: ConvodumpConfig, work_dir: &Path) -> String {
    let state = AppState::new(cfg, engine_info(), work_dir.to_path_buf());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    format!("http://{addr}")
}

fn base_config() -> ConvodumpConfig {
    let mut cfg = ConvodumpConfig::default();
    cfg.server.token = TOKEN.into();
    cfg
}

fn write_script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Stub ffprobe / ffmpeg / nemo-speech that emit the synthetic fixtures.
fn stub_engine(dir: &Path, ffprobe_json: &str, nemo_exit: i32) -> ConvodumpConfig {
    std::fs::write(dir.join("probe.json"), ffprobe_json).unwrap();
    std::fs::write(dir.join("nemo.json"), NEMO_FIXTURE).unwrap();
    std::fs::write(dir.join("diar.rttm"), RTTM_FIXTURE).unwrap();
    write_script(
        &dir.join("ffprobe"),
        &format!("cat {}/probe.json", dir.display()),
    );
    // last argument is the output wav
    write_script(
        &dir.join("ffmpeg"),
        "for a; do last=$a; done; echo wav > \"$last\"",
    );
    // `--output <path>` carries the destination; the subcommand picks which fixture to copy.
    write_script(
        &dir.join("nemo-speech"),
        &format!(
            "if [ {nemo_exit} -ne 0 ]; then echo boom >&2; exit {nemo_exit}; fi\n\
             cmd=$1; shift\n\
             while [ $# -gt 0 ]; do if [ \"$1\" = --output ]; then out=$2; fi; shift; done\n\
             if [ \"$cmd\" = diarize ]; then cp {d}/diar.rttm \"$out\"; else cp {d}/nemo.json \"$out\"; fi",
            d = dir.display()
        ),
    );
    let mut cfg = base_config();
    cfg.engine.ffprobe_bin = dir.join("ffprobe").display().to_string();
    cfg.engine.ffmpeg_bin = dir.join("ffmpeg").display().to_string();
    cfg.engine.nemo_speech_bin = dir.join("nemo-speech").display().to_string();
    cfg
}

fn upload_form() -> reqwest::multipart::Form {
    let part = reqwest::multipart::Part::bytes(b"not really media".to_vec())
        .file_name("../nested/clip.m4a");
    reqwest::multipart::Form::new().part("file", part)
}

#[tokio::test]
async fn health_is_open_and_reports_engine() {
    let work = tempfile::tempdir().unwrap();
    let url = spawn(base_config(), work.path()).await;
    let resp = reqwest::get(format!("{url}/v1/health")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["engine"]["version"], "9.9.9-test");
    assert_eq!(body["engine"]["language"], "auto");
    assert_eq!(body["busy"], false);
}

#[tokio::test]
async fn media_requires_the_right_token() {
    let work = tempfile::tempdir().unwrap();
    let url = spawn(base_config(), work.path()).await;
    let client = reqwest::Client::new();

    let no_token = client
        .post(format!("{url}/v1/media"))
        .multipart(upload_form())
        .send()
        .await
        .unwrap();
    assert_eq!(no_token.status(), 401);
    let body: serde_json::Value = no_token.json().await.unwrap();
    assert_eq!(body["error"]["code"], "unauthorized");

    let wrong = client
        .post(format!("{url}/v1/media"))
        .bearer_auth("wrong-token")
        .multipart(upload_form())
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 401);
}

#[tokio::test]
async fn missing_file_field_is_a_400() {
    let work = tempfile::tempdir().unwrap();
    let url = spawn(base_config(), work.path()).await;
    let form = reqwest::multipart::Form::new().text("language", "en");
    let resp = reqwest::Client::new()
        .post(format!("{url}/v1/media"))
        .bearer_auth(TOKEN)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "missing_file");
}

#[tokio::test]
async fn full_job_returns_document_and_leaves_nothing_behind() {
    let work = tempfile::tempdir().unwrap();
    let stubs = tempfile::tempdir().unwrap();
    let url = spawn(stub_engine(stubs.path(), FFPROBE_FIXTURE, 0), work.path()).await;

    let resp = reqwest::Client::new()
        .post(format!("{url}/v1/media"))
        .bearer_auth(TOKEN)
        .multipart(upload_form().text("language", "en"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let doc: ConvoDump = resp.json().await.unwrap();

    assert_eq!(doc.schema_version, 2);
    assert_eq!(doc.source.file_name, "clip.m4a");
    assert_eq!(doc.source.size_bytes, 16);
    assert_eq!(doc.source.sha256.len(), 64);
    assert_eq!(doc.media.transcribed_stream_index, 1);
    assert_eq!(doc.engine.version, "9.9.9-test");
    assert_eq!(doc.engine.language_requested, "en");
    assert_eq!(doc.engine.diarization_preset, "v3-offline");
    assert_eq!(doc.engine.recognition_mode, "offline");
    // Segments come from the diarizer's turns, not from word spans: three turns, three segments,
    // and every word takes its turn's speaker (the `golf` word included, which the engine left bare).
    assert_eq!(doc.transcript.speakers.len(), 2);
    assert_eq!(doc.transcript.segments.len(), 3);
    let spans: Vec<(&str, &str, f64, f64)> = doc
        .transcript
        .segments
        .iter()
        .map(|s| {
            (
                s.speaker.as_deref().unwrap_or("--"),
                s.text.as_str(),
                s.start_s,
                s.end_s,
            )
        })
        .collect();
    assert_eq!(
        spans,
        vec![
            ("S1", "alpha bravo charlie", 0.0, 1.5),
            ("S1", "delta", 3.8, 4.5),
            ("S2", "echo foxtrot golf", 4.9, 6.5),
        ]
    );
    assert_eq!(doc.transcript.unattributed_word_count, 0);
    assert_eq!(doc.transcript.words[0].speaker.as_deref(), Some("S1"));
    assert_eq!(doc.transcript.words[6].speaker.as_deref(), Some("S2"));
    assert_eq!(
        std::fs::read_dir(work.path()).unwrap().count(),
        0,
        "job dir must be removed"
    );
}

#[tokio::test]
async fn long_media_is_recognized_in_chunks() {
    let work = tempfile::tempdir().unwrap();
    let stubs = tempfile::tempdir().unwrap();
    let url = spawn(
        stub_engine(stubs.path(), FFPROBE_LONG_FIXTURE, 0),
        work.path(),
    )
    .await;

    let resp = reqwest::Client::new()
        .post(format!("{url}/v1/media"))
        .bearer_auth(TOKEN)
        .multipart(upload_form())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let doc: ConvoDump = resp.json().await.unwrap();

    // 1000 s at 390 s per chunk, and no turn start in (60, 390] or (450, 780] -> cuts at 390 and 780.
    assert_eq!(doc.engine.recognition_mode, "offline-chunked");
    assert_eq!(
        doc.transcript.words.len(),
        21,
        "three chunks of the fixture"
    );
    let starts: Vec<f64> = doc
        .transcript
        .words
        .iter()
        .step_by(7)
        .map(|w| w.start_s)
        .collect();
    assert_eq!(starts, vec![0.0, 390.0, 780.0]);
    // The RTTM only covers the first 6.5 s, so the later chunks' words are unattributed.
    assert_eq!(doc.transcript.words[0].speaker.as_deref(), Some("S1"));
    assert_eq!(doc.transcript.words[7].speaker, None);
    assert_eq!(doc.transcript.unattributed_word_count, 14);
}

#[tokio::test]
async fn media_without_audio_is_422() {
    let work = tempfile::tempdir().unwrap();
    let stubs = tempfile::tempdir().unwrap();
    let url = spawn(
        stub_engine(stubs.path(), FFPROBE_NOAUDIO_FIXTURE, 0),
        work.path(),
    )
    .await;
    let resp = reqwest::Client::new()
        .post(format!("{url}/v1/media"))
        .bearer_auth(TOKEN)
        .multipart(upload_form())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "no_audio_stream");
}

#[tokio::test]
async fn engine_failure_is_502_with_stderr_cause() {
    let work = tempfile::tempdir().unwrap();
    let stubs = tempfile::tempdir().unwrap();
    let url = spawn(stub_engine(stubs.path(), FFPROBE_FIXTURE, 3), work.path()).await;
    let resp = reqwest::Client::new()
        .post(format!("{url}/v1/media"))
        .bearer_auth(TOKEN)
        .multipart(upload_form())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 502);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "engine_failed");
    assert_eq!(body["error"]["cause"], "boom");
    assert_eq!(std::fs::read_dir(work.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn oversize_upload_is_413() {
    let work = tempfile::tempdir().unwrap();
    let mut cfg = base_config();
    cfg.server.max_upload_mb = 1;
    let url = spawn(cfg, work.path()).await;
    let part = reqwest::multipart::Part::bytes(vec![0u8; 2 * 1024 * 1024]).file_name("big.wav");
    let resp = reqwest::Client::new()
        .post(format!("{url}/v1/media"))
        .bearer_auth(TOKEN)
        .multipart(reqwest::multipart::Form::new().part("file", part))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 413);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "too_large");
}
