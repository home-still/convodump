use convodump_core::models::ConvoDump;
use convodump_core::timecode::timecode;

/// Placeholder shown where a segment has no speaker.
const NO_SPEAKER: &str = "--";

/// Header line plus one line per segment: `[start → end] speaker: text`.
pub fn render_text(doc: &ConvoDump) -> String {
    let t = &doc.transcript;
    let mut out = format!(
        "{}  duration {}  speakers {}  language {}\n",
        doc.source.file_name,
        timecode(t.duration_s),
        t.speakers.len(),
        t.languages.join(",")
    );
    for s in &t.segments {
        // A turn whose speech was not recognized still exists in the document; it has nothing to print.
        if s.text.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "[{} → {}] {}: {}\n",
            s.start_tc,
            s.end_tc,
            s.speaker.as_deref().unwrap_or(NO_SPEAKER),
            s.text
        ));
    }
    out
}
