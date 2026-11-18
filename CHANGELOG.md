# Changelog

## [0.2.0] - 2026-11-18

### Changed
- Recognition now runs in offline (full-attention) chunks of at most `engine.offline_chunk_max_s`
  (390 s, the engine's own cutoff) instead of one cache-aware streaming pass over long audio, which
  measured 0.1269 -> 0.118 WER on the reference memo.
- Diarization runs as its own `nemo-speech diarize` pass (`engine.diarization_preset`, default
  `v3-offline`); its turns are the transcript's segments and its word-to-turn overlap assigns
  speakers, instead of relying on recognizer word spans. Measured 0.273 -> 0.118 DER on the memo and
  0.233 -> 0.133 on AMI ES2015b, and it removed a spurious extra speaker.
- Schema version 2: `timing.diarize_ms`, `engine.diarization_preset`, `engine.recognition_mode`.

## [0.1.0] - 2026-11-18

### Added
- `convodump media run`: upload audio or video files to a ConvoDump server and print a
  diarized, time-coded transcript or write `<stem>.convodump.json` (schema version 1:
  media metadata, speakers, segments, words).
- `convodump server run` / `server status`: HTTP API (`/v1/health`, `/v1/media`) over
  ffprobe, ffmpeg and NeMo-Speech.cpp 0.3.0 (`nemotron-3.5-asr-streaming-0.6b`,
  `Nemotron-3-Diarization`); one job at a time, bearer-token auth, nothing kept after a job.
- `engine.wrapper` argv prefix for GPU schedulers.
- `scripts/privacy-scan.sh` with pre-commit / pre-push hooks and a CI job.
- Verified on a one-hour file: 3600 s transcribed in about 7.3 minutes with a peak engine
  resident set of about 2.3 GiB, so `--stream` is not used.
