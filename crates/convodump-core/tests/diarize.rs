use convodump_core::diarize::{parse_rttm, Turn};

#[test]
fn two_speakers_in_order() {
    let text = "SPEAKER job 1 0.000 1.500 <NA> <NA> speaker_1 <NA> <NA>\n\
                SPEAKER job 1 4.900 1.600 <NA> <NA> speaker_2 <NA> <NA>\n";
    assert_eq!(
        parse_rttm(text),
        vec![
            Turn {
                start_s: 0.0,
                end_s: 1.5,
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

#[test]
fn lines_are_sorted_by_start() {
    let text = "SPEAKER job 1 5.000 1.000 <NA> <NA> b <NA> <NA>\n\
                SPEAKER job 1 1.000 1.000 <NA> <NA> a <NA> <NA>\n";
    let turns = parse_rttm(text);
    assert_eq!(
        turns.iter().map(|t| t.start_s).collect::<Vec<_>>(),
        vec![1.0, 5.0]
    );
    assert_eq!(turns[0].speaker, "a");
}

#[test]
fn non_positive_duration_and_foreign_lines_are_skipped() {
    let text = "SPEAKER job 1 1.000 0.000 <NA> <NA> a <NA> <NA>\n\
                SPEAKER job 1 2.000 -1.000 <NA> <NA> a <NA> <NA>\n\
                SEGMENT job 1 3.000 1.000 <NA> <NA> a <NA> <NA>\n\
                SPEAKER job 1 4.000 1.000 <NA> <NA> b <NA> <NA>\n";
    let turns = parse_rttm(text);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].start_s, 4.0);
}

#[test]
fn empty_input_is_empty_output() {
    assert!(parse_rttm("").is_empty());
    assert!(parse_rttm("\n\n").is_empty());
}
