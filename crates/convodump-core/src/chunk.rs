//! Cut planning for offline recognition.
//!
//! The engine runs full-attention recognition up to 390 s of audio and silently switches to
//! cache-aware streaming above that, so longer files are split into chunks and transcribed one at
//! a time. Cuts land on a diarizer turn start: the diarizer only starts a turn where it saw speech
//! begin, so a cut there does not land inside a spoken word.

use crate::diarize::Turn;

/// Shortest acceptable final chunk; a window is only closed early if the remainder can still stand alone.
pub const MIN_TAIL_S: f64 = 60.0;

/// Inclusive `(start_s, end_s)` spans covering `0.0 ..= duration_s`.
pub fn plan_cuts(turns: &[Turn], duration_s: f64, max_chunk_s: f64) -> Vec<(f64, f64)> {
    if !(duration_s.is_finite()) || duration_s <= 0.0 {
        return Vec::new();
    }
    if !(max_chunk_s.is_finite()) || max_chunk_s <= 0.0 {
        return vec![(0.0, duration_s)];
    }

    let mut spans: Vec<(f64, f64)> = Vec::new();
    let mut current = 0.0;
    while duration_s - current > max_chunk_s {
        let window_lo = current + MIN_TAIL_S;
        let window_hi = current + max_chunk_s;
        let cut = turns
            .iter()
            .map(|t| t.start_s)
            .filter(|s| *s > window_lo && *s <= window_hi)
            .fold(None::<f64>, |acc, s| Some(acc.map_or(s, |a: f64| a.max(s))))
            .unwrap_or(window_hi);
        spans.push((current, cut));
        current = cut;
    }
    spans.push((current, duration_s));
    spans
}
