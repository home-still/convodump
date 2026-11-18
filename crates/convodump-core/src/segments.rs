//! Words -> speaker segments -> [`Transcript`].
//!
//! Two segment sources. With diarizer turns, each turn is one segment and the turn labels define the
//! `S1..Sn` ids; the word spans alone cannot serve, because a recognizer emits no span for the
//! silence inside a turn, so word-driven segments annotate far less speech than was spoken. Without
//! turns (a diarizer that found nothing), words are grouped by speaker change and
//! [`SEGMENT_PAUSE_SPLIT_S`], which is what schema 1 did.

use crate::diarize::Turn;
use crate::engine::{EngineTranscript, EngineWord};
use crate::models::{Segment, Speaker, Transcript, Word};
use crate::timecode::{round_s, timecode};

/// A pause longer than this inside one speaker's words opens a new segment.
pub const SEGMENT_PAUSE_SPLIT_S: f64 = 2.0;

/// A word must overlap a turn by at least this much to take its speaker.
pub const WORD_SPEAKER_MIN_OVERLAP_S: f64 = 0.0;

pub fn build_transcript(e: EngineTranscript) -> Transcript {
    build_transcript_with_turns(e, &[])
}

pub fn build_transcript_with_turns(e: EngineTranscript, turns: &[Turn]) -> Transcript {
    let mut engine_words = e.words;
    engine_words.sort_by(|a, b| {
        a.start
            .total_cmp(&b.start)
            .then_with(|| a.end.total_cmp(&b.end))
    });

    // Turn labels in turn order -> `S1..Sn`; a label reused later keeps the id it first took.
    let mut labels: Vec<&str> = Vec::new();
    for t in turns {
        if !labels.contains(&t.speaker.as_str()) {
            labels.push(&t.speaker);
        }
    }
    let speaker_id = |label: &str| -> Option<String> {
        labels
            .iter()
            .position(|l| *l == label)
            .map(|i| format!("S{}", i + 1))
    };

    let word_speakers: Vec<Option<String>> = if turns.is_empty() {
        engine_words
            .iter()
            .map(|w| w.speaker.map(|n| format!("S{n}")))
            .collect()
    } else {
        attribute_by_turn(&engine_words, turns, &speaker_id)
    };

    let words: Vec<Word> = engine_words
        .into_iter()
        .zip(word_speakers)
        .enumerate()
        .map(|(i, (w, speaker))| Word {
            index: i as u32,
            text: w.word,
            start_s: round_s(w.start),
            end_s: round_s(w.end),
            confidence: w.confidence,
            speaker,
        })
        .collect();

    let unattributed_word_count = words.iter().filter(|w| w.speaker.is_none()).count() as u32;

    let segments = if turns.is_empty() {
        group_by_pause(&words)
    } else {
        segment_per_turn(&words, turns, &speaker_id)
    };

    let mut speakers: Vec<Speaker> = Vec::new();
    for w in &words {
        let Some(id) = &w.speaker else { continue };
        let idx = match speakers.iter().position(|s| &s.id == id) {
            Some(idx) => idx,
            None => {
                speakers.push(Speaker {
                    id: id.clone(),
                    word_count: 0,
                    segment_count: 0,
                    talk_time_s: 0.0,
                    first_start_s: w.start_s,
                });
                speakers.len() - 1
            }
        };
        speakers[idx].word_count += 1;
        speakers[idx].talk_time_s += w.end_s - w.start_s;
    }
    for seg in &segments {
        if let Some(id) = &seg.speaker {
            if let Some(sp) = speakers.iter_mut().find(|s| &s.id == id) {
                sp.segment_count += 1;
            }
        }
    }
    for sp in &mut speakers {
        sp.talk_time_s = round_s(sp.talk_time_s);
    }

    Transcript {
        text: e.text,
        languages: e.languages,
        duration_s: round_s(e.duration),
        confidence: e.confidence,
        speakers,
        segments,
        words,
        unattributed_word_count,
    }
}

/// Each word takes the turn it overlaps longest; a word that overlaps none stays unattributed.
///
/// Words and turns are both sorted by start, so a cursor over the turns never has to go back: a
/// turn that ends before this word starts can never overlap this or any later word.
fn attribute_by_turn(
    words: &[EngineWord],
    turns: &[Turn],
    speaker_id: &dyn Fn(&str) -> Option<String>,
) -> Vec<Option<String>> {
    let mut out = Vec::with_capacity(words.len());
    let mut cursor = 0usize;
    for w in words {
        while cursor < turns.len() && turns[cursor].end_s <= w.start {
            cursor += 1;
        }
        let mut best: Option<(f64, &Turn)> = None;
        for t in &turns[cursor..] {
            if t.start_s >= w.end {
                break;
            }
            let overlap = w.end.min(t.end_s) - w.start.max(t.start_s);
            if overlap <= WORD_SPEAKER_MIN_OVERLAP_S {
                continue;
            }
            if best.is_none_or(|(best_overlap, _)| overlap > best_overlap) {
                best = Some((overlap, t));
            }
        }
        out.push(best.and_then(|(_, t)| speaker_id(&t.speaker)));
    }
    out
}

/// One segment per turn — every turn, including one whose speech was not recognized. The document's
/// segments are the speech timeline a reader or a scorer measures (DER), so dropping a wordless turn
/// would erase speech that was spoken. A word inside two overlapping turns belongs to the first, so no
/// word is counted twice.
fn segment_per_turn(
    words: &[Word],
    turns: &[Turn],
    speaker_id: &dyn Fn(&str) -> Option<String>,
) -> Vec<Segment> {
    let mut segments: Vec<Segment> = Vec::new();
    let mut cursor = 0usize;
    for turn in turns {
        let mut first_word: Option<u32> = None;
        let mut last_word: u32 = 0;
        let mut text = String::new();
        while cursor < words.len() {
            let w = &words[cursor];
            let midpoint = (w.start_s + w.end_s) / 2.0;
            if midpoint < turn.start_s {
                cursor += 1;
                continue;
            }
            if midpoint > turn.end_s {
                break;
            }
            if first_word.is_none() {
                first_word = Some(w.index);
            }
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&w.text);
            last_word = w.index;
            cursor += 1;
        }
        let last_word = first_word.map(|_| last_word);
        segments.push(Segment {
            index: segments.len() as u32,
            speaker: speaker_id(&turn.speaker),
            start_s: round_s(turn.start_s),
            end_s: round_s(turn.end_s),
            start_tc: timecode(round_s(turn.start_s)),
            end_tc: timecode(round_s(turn.end_s)),
            text,
            first_word,
            last_word,
        });
    }
    segments
}

/// Word-driven grouping: a new segment on a speaker change or a pause over the threshold.
fn group_by_pause(words: &[Word]) -> Vec<Segment> {
    let mut segments: Vec<Segment> = Vec::new();
    for (i, w) in words.iter().enumerate() {
        let i = i as u32;
        let continues = segments.last().is_some_and(|s| {
            s.speaker == w.speaker
                && w.start_s - words[s.last_word.unwrap_or_default() as usize].end_s
                    <= SEGMENT_PAUSE_SPLIT_S
        });
        if continues {
            let seg = segments
                .last_mut()
                .expect("continues implies a last segment");
            seg.end_s = seg.end_s.max(w.end_s);
            seg.end_tc = timecode(seg.end_s);
            seg.text.push(' ');
            seg.text.push_str(&w.text);
            seg.last_word = Some(i);
        } else {
            segments.push(Segment {
                index: segments.len() as u32,
                speaker: w.speaker.clone(),
                start_s: w.start_s,
                end_s: w.end_s,
                start_tc: timecode(w.start_s),
                end_tc: timecode(w.end_s),
                text: w.text.clone(),
                first_word: Some(i),
                last_word: Some(i),
            });
        }
    }
    segments
}
