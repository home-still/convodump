//! `ffprobe -print_format json` -> [`Media`].

use std::collections::BTreeMap;

use serde_json::Value;

use crate::error::{ConvodumpError, Result};
use crate::models::Media;

fn number_field<T: std::str::FromStr>(obj: &Value, key: &str) -> Option<T> {
    match obj.get(key)? {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.to_string().parse().ok(),
        _ => None,
    }
}

/// Parse ffprobe JSON. `format.filename` (the server's temp path) is dropped.
pub fn parse_ffprobe(json: &str) -> Result<Media> {
    let root: Value = serde_json::from_str(json).map_err(|e| ConvodumpError::UnreadableMedia {
        stderr_tail: format!("ffprobe JSON is invalid: {e}"),
    })?;
    let format = root
        .get("format")
        .ok_or_else(|| ConvodumpError::UnreadableMedia {
            stderr_tail: "ffprobe output has no `format` object".into(),
        })?;
    let streams = root
        .get("streams")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let chapters = root
        .get("chapters")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let transcribed_stream_index = streams
        .iter()
        .find(|s| s.get("codec_type").and_then(Value::as_str) == Some("audio"))
        .and_then(|s| s.get("index").and_then(Value::as_u64))
        .ok_or(ConvodumpError::NoAudioStream)? as u32;

    let tags: BTreeMap<String, String> = format
        .get("tags")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_owned())))
                .collect()
        })
        .unwrap_or_default();

    Ok(Media {
        format_name: format
            .get("format_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        format_long_name: format
            .get("format_long_name")
            .and_then(Value::as_str)
            .map(str::to_owned),
        duration_s: number_field(format, "duration"),
        size_bytes: number_field(format, "size"),
        bit_rate: number_field(format, "bit_rate"),
        tags,
        streams,
        chapters,
        transcribed_stream_index,
    })
}
