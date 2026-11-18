use thiserror::Error;

/// Number of trailing characters of a child process's stderr kept in an error.
pub const STDERR_TAIL_CHARS: usize = 2000;

#[derive(Debug, Error)]
pub enum ConvodumpError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("the media has no audio stream")]
    NoAudioStream,
    #[error("ffprobe could not read the media: {stderr_tail}")]
    UnreadableMedia { stderr_tail: String },
    #[error("ffmpeg could not decode the audio: {stderr_tail}")]
    Decode { stderr_tail: String },
    #[error("nemo-speech failed: {stderr_tail}")]
    Engine { stderr_tail: String },
    #[error("nemo-speech produced invalid output: {0}")]
    EngineOutput(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ConvodumpError>;

/// Last [`STDERR_TAIL_CHARS`] characters of `stderr`, trimmed.
pub fn stderr_tail(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let text = text.trim();
    let count = text.chars().count();
    text.chars()
        .skip(count.saturating_sub(STDERR_TAIL_CHARS))
        .collect()
}
