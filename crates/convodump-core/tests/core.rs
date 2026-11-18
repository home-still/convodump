use convodump_core::diarize::Turn;
use convodump_core::engine::parse_nemo_json;
use convodump_core::error::ConvodumpError;
use convodump_core::probe::parse_ffprobe;
use convodump_core::segments::{build_transcript, build_transcript_with_turns};
use convodump_core::timecode::timecode;

const TWO_SPEAKERS: &str = include_str!("fixtures/nemo_two_speakers.json");
const FFPROBE_MP4: &str = include_str!("fixtures/ffprobe_mp4.json");
const FFPROBE_NOAUDIO: &str = include_str!("fixtures/ffprobe_noaudio.json");

#[test]
fn two_speakers_segments_and_stats() {
    let t = build_transcript(parse_nemo_json(TWO_SPEAKERS).unwrap());

    let spans: Vec<(Option<&str>, &str, f64, f64)> = t
        .segments
        .iter()
        .map(|s| (s.speaker.as_deref(), s.text.as_str(), s.start_s, s.end_s))
        .collect();
    assert_eq!(
        spans,
        vec![
            (Some("S1"), "alpha bravo charlie", 0.0, 1.4),
            // same speaker, 2.5 s pause -> split
            (Some("S1"), "delta", 3.9, 4.3),
            (Some("S2"), "echo foxtrot", 5.0, 5.9),
            (None, "golf", 6.0, 6.4),
        ]
    );
    assert_eq!(t.segments[0].start_tc, "00:00:00.000");
    assert_eq!(t.segments[2].end_tc, "00:00:05.900");
    assert_eq!(
        (t.segments[0].first_word, t.segments[0].last_word),
        (Some(0), Some(2))
    );

    assert_eq!(t.unattributed_word_count, 1);
    assert_eq!(t.speakers.len(), 2);
    let s1 = &t.speakers[0];
    assert_eq!(
        (s1.id.as_str(), s1.word_count, s1.segment_count),
        ("S1", 4, 2)
    );
    assert_eq!(s1.talk_time_s, 1.6);
    assert_eq!(s1.first_start_s, 0.0);
    let s2 = &t.speakers[1];
    assert_eq!(
        (s2.id.as_str(), s2.word_count, s2.segment_count),
        ("S2", 2, 1)
    );
    assert_eq!(s2.talk_time_s, 0.8);
    // words are sorted by start time
    assert_eq!(t.words[1].text, "bravo");
}

#[test]
fn text_without_words_is_rejected() {
    let json = r#"{"text":"alpha","confidence":0.9,"duration":1.0,"languages":["en"],"words":[]}"#;
    assert!(matches!(
        parse_nemo_json(json),
        Err(ConvodumpError::EngineOutput(_))
    ));
}

#[test]
fn negative_start_is_rejected() {
    let json =
        r#"{"text":"alpha","duration":1.0,"words":[{"word":"alpha","start":-0.1,"end":0.4}]}"#;
    assert!(matches!(
        parse_nemo_json(json),
        Err(ConvodumpError::EngineOutput(_))
    ));
}

#[test]
fn end_before_start_is_rejected() {
    let json =
        r#"{"text":"alpha","duration":1.0,"words":[{"word":"alpha","start":0.5,"end":0.4}]}"#;
    assert!(matches!(
        parse_nemo_json(json),
        Err(ConvodumpError::EngineOutput(_))
    ));
}

#[test]
fn silence_is_an_empty_transcript() {
    let json = r#"{"text":"","confidence":0.0,"duration":3.0,"languages":[],"words":[]}"#;
    let t = build_transcript(parse_nemo_json(json).unwrap());
    assert!(t.segments.is_empty() && t.speakers.is_empty() && t.words.is_empty());
    assert_eq!(t.duration_s, 3.0);
}

#[test]
fn ffprobe_mp4_picks_audio_stream_and_drops_filename() {
    let m = parse_ffprobe(FFPROBE_MP4).unwrap();
    assert_eq!(m.transcribed_stream_index, 1);
    assert_eq!(m.duration_s, Some(12.345));
    assert_eq!(m.size_bytes, Some(123456));
    assert_eq!(m.bit_rate, Some(80000));
    assert_eq!(m.tags.get("title").map(String::as_str), Some("Sample"));
    let json = serde_json::to_string(&m).unwrap();
    assert!(!json.contains("filename") && !json.contains("job-1234"));
}

#[test]
fn ffprobe_without_audio_is_rejected() {
    assert!(matches!(
        parse_ffprobe(FFPROBE_NOAUDIO),
        Err(ConvodumpError::NoAudioStream)
    ));
}

#[test]
fn timecode_format() {
    assert_eq!(timecode(3661.0005), "01:01:01.001");
    assert_eq!(timecode(0.0), "00:00:00.000");
    assert_eq!(timecode(360000.5), "100:00:00.500");
}

#[test]
fn turns_become_segments_and_carry_the_speaker() {
    // Two turns over the same fixture: the first covers alpha..charlie, the second echo..golf.
    let turns = vec![
        Turn {
            start_s: 0.0,
            end_s: 1.4,
            speaker: "speaker_1".into(),
        },
        Turn {
            start_s: 4.9,
            end_s: 6.5,
            speaker: "speaker_2".into(),
        },
    ];
    let t = build_transcript_with_turns(parse_nemo_json(TWO_SPEAKERS).unwrap(), &turns);

    let spans: Vec<(Option<&str>, &str, f64, f64)> = t
        .segments
        .iter()
        .map(|s| (s.speaker.as_deref(), s.text.as_str(), s.start_s, s.end_s))
        .collect();
    assert_eq!(
        spans,
        vec![
            (Some("S1"), "alpha bravo charlie", 0.0, 1.4),
            (Some("S2"), "echo foxtrot golf", 4.9, 6.5),
        ]
    );
    // `delta` at 3.9-4.3 s falls in neither turn, so the turn-driven path leaves it unattributed.
    assert_eq!(t.words[3].text, "delta");
    assert_eq!(t.words[3].speaker, None);
    assert_eq!(t.unattributed_word_count, 1);
    // The engine's own word labels are ignored: `golf` had none and `echo` said speaker 2.
    assert_eq!(t.words[0].speaker.as_deref(), Some("S1"));
    assert_eq!(t.words[6].speaker.as_deref(), Some("S2"));

    assert_eq!(t.segments[0].start_tc, "00:00:00.000");
    assert_eq!(t.segments[1].end_tc, "00:00:06.500");
    assert_eq!(
        (t.segments[1].first_word, t.segments[1].last_word),
        (Some(4), Some(6))
    );

    assert_eq!(t.speakers.len(), 2);
    assert_eq!(
        (
            t.speakers[0].id.as_str(),
            t.speakers[0].word_count,
            t.speakers[0].segment_count
        ),
        ("S1", 3, 1)
    );
    assert_eq!(
        (
            t.speakers[1].id.as_str(),
            t.speakers[1].word_count,
            t.speakers[1].segment_count
        ),
        ("S2", 3, 1)
    );
}

#[test]
fn no_turns_falls_back_to_the_word_driven_segments() {
    let with_turns = build_transcript_with_turns(parse_nemo_json(TWO_SPEAKERS).unwrap(), &[]);
    assert_eq!(
        with_turns,
        build_transcript(parse_nemo_json(TWO_SPEAKERS).unwrap())
    );
    assert_eq!(with_turns.segments.len(), 4);
}

#[test]
fn overlapping_turns_give_a_word_to_the_first_of_them() {
    // A word inside two turns is counted once, in the earlier segment, so text is never duplicated.
    let json = r#"{"text":"alpha bravo","duration":3.0,"words":[
        {"word":"alpha","start":0.2,"end":0.6},
        {"word":"bravo","start":1.2,"end":1.6}]}"#;
    let turns = vec![
        Turn {
            start_s: 0.0,
            end_s: 1.0,
            speaker: "speaker_2".into(),
        },
        Turn {
            start_s: 0.5,
            end_s: 2.0,
            speaker: "speaker_1".into(),
        },
    ];
    let t = build_transcript_with_turns(parse_nemo_json(json).unwrap(), &turns);
    let texts: Vec<(&str, &str)> = t
        .segments
        .iter()
        .map(|s| (s.speaker.as_deref().unwrap_or("--"), s.text.as_str()))
        .collect();
    assert_eq!(texts, vec![("S1", "alpha"), ("S2", "bravo")]);
}

#[test]
fn a_turn_without_words_still_marks_the_speech_timeline() {
    // The middle turn spans a gap with no recognized word. It stays a segment with empty text and no
    // word indices: the segment list is the speech timeline a scorer measures (DER), so dropping it
    // would erase speech that was spoken.
    let turns = vec![
        Turn {
            start_s: 0.0,
            end_s: 1.4,
            speaker: "speaker_1".into(),
        },
        Turn {
            start_s: 1.5,
            end_s: 3.0,
            speaker: "speaker_2".into(),
        },
        Turn {
            start_s: 4.9,
            end_s: 6.5,
            speaker: "speaker_2".into(),
        },
    ];
    let t = build_transcript_with_turns(parse_nemo_json(TWO_SPEAKERS).unwrap(), &turns);
    let rows: Vec<(u32, &str, &str, f64, f64)> = t
        .segments
        .iter()
        .map(|s| {
            (
                s.index,
                s.speaker.as_deref().unwrap_or("--"),
                s.text.as_str(),
                s.start_s,
                s.end_s,
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            (0, "S1", "alpha bravo charlie", 0.0, 1.4),
            (1, "S2", "", 1.5, 3.0),
            (2, "S2", "echo foxtrot golf", 4.9, 6.5),
        ]
    );
    assert_eq!(t.segments[1].first_word, None);
    assert_eq!(t.segments[1].last_word, None);
    assert_eq!(t.segments[2].first_word, Some(4));
    assert_eq!(t.speakers[1].segment_count, 2);
}

#[test]
fn one_turn_spanning_the_file_keeps_every_word_in_one_segment() {
    let turns = vec![Turn {
        start_s: 0.0,
        end_s: 7.0,
        speaker: "speaker_1".into(),
    }];
    let t = build_transcript_with_turns(parse_nemo_json(TWO_SPEAKERS).unwrap(), &turns);
    assert_eq!(t.segments.len(), 1);
    assert_eq!(
        t.segments[0].text,
        "alpha bravo charlie delta echo foxtrot golf"
    );
    assert_eq!(t.speakers.len(), 1);
    assert_eq!(t.unattributed_word_count, 0);
}
