const MS_PER_SECOND: f64 = 1000.0;
const MS_PER_MINUTE: u64 = 60_000;
const MS_PER_HOUR: u64 = 3_600_000;

/// `HH:MM:SS.mmm`; hours are unbounded. Input must be finite and >= 0
/// (`engine::parse_nemo_json` guarantees this).
pub fn timecode(seconds: f64) -> String {
    let total_ms = (seconds * MS_PER_SECOND).round() as u64;
    let hours = total_ms / MS_PER_HOUR;
    let minutes = (total_ms % MS_PER_HOUR) / MS_PER_MINUTE;
    let secs = (total_ms % MS_PER_MINUTE) / 1000;
    let ms = total_ms % 1000;
    format!("{hours:02}:{minutes:02}:{secs:02}.{ms:03}")
}

/// Round to millisecond precision (3 decimals).
pub fn round_s(seconds: f64) -> f64 {
    (seconds * MS_PER_SECOND).round() / MS_PER_SECOND
}
