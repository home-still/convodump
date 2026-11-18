//! ConvoDump core: the output schema and the pure logic behind it.
//!
//! Pipeline: `ffprobe` -> `ffmpeg` decode -> `nemo-speech` -> [`segments::build_transcript`].

pub mod config;
pub mod engine;
pub mod error;
pub mod models;
pub mod pipeline;
pub mod probe;
pub mod segments;
pub mod timecode;

pub use error::{ConvodumpError, Result};
