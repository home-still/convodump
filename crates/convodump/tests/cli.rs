use std::process::Command;

use convodump::output::render_text;
use convodump_core::engine::parse_nemo_json;
use convodump_core::models::{ConvoDump, EngineInfo, Media, Source, Timing};
use convodump_core::segments::build_transcript;

const NEMO_FIXTURE: &str =
    include_str!("../../convodump-core/tests/fixtures/nemo_two_speakers.json");

#[test]
fn render_text_lists_header_and_segments() {
    let doc = ConvoDump {
        schema_version: 1,
        convodump_version: "0.1.0".into(),
        source: Source {
            file_name: "clip.m4a".into(),
            size_bytes: 1,
            sha256: "0".repeat(64),
        },
        media: Media {
            format_name: "mov".into(),
            format_long_name: None,
            duration_s: Some(7.0),
            size_bytes: None,
            bit_rate: None,
            tags: Default::default(),
            streams: vec![],
            chapters: vec![],
            transcribed_stream_index: 0,
        },
        engine: EngineInfo {
            name: "nemo-speech".into(),
            version: "x".into(),
            asr_model: "a".into(),
            diarization_model: "d".into(),
            language_requested: "auto".into(),
        },
        transcript: build_transcript(parse_nemo_json(NEMO_FIXTURE).unwrap()),
        timing: Timing {
            probe_ms: 0,
            decode_ms: 0,
            engine_ms: 0,
            total_ms: 0,
        },
    };
    assert_eq!(
        render_text(&doc),
        "clip.m4a  duration 00:00:07.000  speakers 2  language en\n\
         [00:00:00.000 → 00:00:01.400] S1: alpha bravo charlie\n\
         [00:00:03.900 → 00:00:04.300] S1: delta\n\
         [00:00:05.000 → 00:00:05.900] S2: echo foxtrot\n\
         [00:00:06.000 → 00:00:06.400] --: golf\n"
    );
}

fn convodump(config_dir: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_convodump"));
    cmd.env("HOME_STILL_CONFIG_DIR", config_dir)
        .env_remove("HOME_STILL_CONVODUMP_CLIENT__SERVER_URL")
        .env_remove("HOME_STILL_CONVODUMP_CLIENT__TOKEN");
    cmd
}

#[test]
fn closed_port_exits_3() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("clip.m4a");
    std::fs::write(&file, b"x").unwrap();
    let out = convodump(dir.path())
        .env(
            "HOME_STILL_CONVODUMP_CLIENT__SERVER_URL",
            "http://127.0.0.1:1",
        )
        .env("HOME_STILL_CONVODUMP_CLIENT__TOKEN", "t")
        .args(["media", "run"])
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr)
        .contains("cannot reach convodump server at http://127.0.0.1:1"));
}

#[test]
fn missing_token_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("clip.m4a");
    std::fs::write(&file, b"x").unwrap();
    let out = convodump(dir.path())
        .args(["media", "run"])
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn missing_input_file_exits_1() {
    let dir = tempfile::tempdir().unwrap();
    let out = convodump(dir.path())
        .env("HOME_STILL_CONVODUMP_CLIENT__TOKEN", "t")
        .args(["media", "run"])
        .arg(dir.path().join("nonexistent.m4a"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}
