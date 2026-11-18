use convodump_core::chunk::{plan_cuts, MIN_TAIL_S};
use convodump_core::diarize::{parse_rttm, Turn};

fn turn(start_s: f64, end_s: f64) -> Turn {
    Turn {
        start_s,
        end_s,
        speaker: "speaker_1".into(),
    }
}

const MEMO_DURATION_S: f64 = 1091.392;

#[test]
fn cuts_land_on_turn_starts() {
    // The memo's real shape: turn starts at 389.89 s and 761.57 s.
    let turns = vec![turn(0.0, 5.0), turn(389.89, 393.0), turn(761.57, 765.0)];
    let spans = plan_cuts(&turns, MEMO_DURATION_S, 390.0);
    assert_eq!(
        spans,
        vec![(0.0, 389.89), (389.89, 761.57), (761.57, MEMO_DURATION_S)]
    );
}

#[test]
fn latest_turn_start_in_the_window_wins() {
    let turns = vec![turn(100.0, 110.0), turn(300.0, 310.0), turn(370.0, 380.0)];
    let spans = plan_cuts(&turns, 1000.0, 390.0);
    assert_eq!(spans[0], (0.0, 370.0));
}

#[test]
fn a_turn_start_too_close_to_the_chunk_start_is_skipped() {
    // 30 s in is below MIN_TAIL_S, so it must not open a 30 s first chunk.
    let turns = vec![turn(30.0, 40.0)];
    let spans = plan_cuts(&turns, 1000.0, 390.0);
    assert_eq!(spans[0], (0.0, 390.0));
    const { assert!(MIN_TAIL_S > 30.0) };
}

#[test]
fn without_a_turn_in_the_window_the_cut_is_the_limit() {
    let turns = vec![turn(0.0, 5.0), turn(20.0, 25.0)];
    let spans = plan_cuts(&turns, 1000.0, 390.0);
    assert_eq!(spans, vec![(0.0, 390.0), (390.0, 780.0), (780.0, 1000.0)]);
}

#[test]
fn short_audio_is_one_span() {
    let turns = vec![turn(0.0, 5.0), turn(20.0, 25.0)];
    assert_eq!(plan_cuts(&turns, 100.0, 390.0), vec![(0.0, 100.0)]);
}

#[test]
fn zero_max_is_one_pass_over_the_whole_file() {
    let turns = vec![turn(200.0, 210.0)];
    assert_eq!(plan_cuts(&turns, 1000.0, 0.0), vec![(0.0, 1000.0)]);
}

#[test]
fn no_turns_still_covers_the_file() {
    assert_eq!(
        plan_cuts(&[], 800.0, 390.0),
        vec![(0.0, 390.0), (390.0, 780.0), (780.0, 800.0)]
    );
}

#[test]
fn unknown_duration_is_no_plan() {
    assert!(plan_cuts(&[], 0.0, 390.0).is_empty());
    assert!(plan_cuts(&[], f64::NAN, 390.0).is_empty());
}

#[test]
fn spans_are_contiguous_and_cover_the_file() {
    let turns: Vec<Turn> = (0..40)
        .map(|i| turn(i as f64 * 37.0, i as f64 * 37.0 + 30.0))
        .collect();
    let spans = plan_cuts(&turns, MEMO_DURATION_S, 390.0);
    assert!(spans.iter().all(|(a, b)| b > a && *b - *a <= 390.0));
    for pair in spans.windows(2) {
        assert_eq!(pair[0].1, pair[1].0);
    }
    assert_eq!(spans.first().unwrap().0, 0.0);
    assert_eq!(spans.last().unwrap().1, MEMO_DURATION_S);
}

#[test]
fn rttm_turns_are_parsed_and_sorted() {
    let text = include_str!("fixtures/diar_two_speakers.rttm");
    let turns = parse_rttm(text);
    assert_eq!(
        turns,
        vec![
            Turn {
                start_s: 0.0,
                end_s: 1.5,
                speaker: "speaker_1".into()
            },
            Turn {
                start_s: 3.8,
                end_s: 4.5,
                speaker: "speaker_1".into()
            },
            Turn {
                start_s: 4.9,
                end_s: 6.5,
                speaker: "speaker_2".into()
            },
        ]
    );
}
