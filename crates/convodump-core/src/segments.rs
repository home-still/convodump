//! Words -> speaker segments -> [`Transcript`].

use crate::engine::EngineTranscript;
use crate::models::{Segment, Speaker, Transcript, Word};
use crate::timecode::{round_s, timecode};

/// A pause longer than this inside one speaker's words opens a new segment.
pub const SEGMENT_PAUSE_SPLIT_S: f64 = 2.0;

pub fn build_transcript(e: EngineTranscript) -> Transcript {
    let mut engine_words = e.words;
    engine_words.sort_by(|a, b| {
        a.start
            .total_cmp(&b.start)
            .then_with(|| a.end.total_cmp(&b.end))
    });

    let words: Vec<Word> = engine_words
        .into_iter()
        .enumerate()
        .map(|(i, w)| Word {
            index: i as u32,
            text: w.word,
            start_s: round_s(w.start),
            end_s: round_s(w.end),
            confidence: w.confidence,
            speaker: w.speaker.map(|n| format!("S{n}")),
        })
        .collect();

    let unattributed_word_count = words.iter().filter(|w| w.speaker.is_none()).count() as u32;

    let mut segments: Vec<Segment> = Vec::new();
    for (i, w) in words.iter().enumerate() {
        let i = i as u32;
        let continues = segments.last().is_some_and(|s| {
            s.speaker == w.speaker
                && w.start_s - words[s.last_word as usize].end_s <= SEGMENT_PAUSE_SPLIT_S
        });
        if continues {
            let seg = segments
                .last_mut()
                .expect("continues implies a last segment");
            seg.end_s = seg.end_s.max(w.end_s);
            seg.end_tc = timecode(seg.end_s);
            seg.text.push(' ');
            seg.text.push_str(&w.text);
            seg.last_word = i;
        } else {
            segments.push(Segment {
                index: segments.len() as u32,
                speaker: w.speaker.clone(),
                start_s: w.start_s,
                end_s: w.end_s,
                start_tc: timecode(w.start_s),
                end_tc: timecode(w.end_s),
                text: w.text.clone(),
                first_word: i,
                last_word: i,
            });
        }
    }

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
