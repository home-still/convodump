# Changelog

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
