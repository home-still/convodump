# ConvoDump

One job: take an audio or video file, return one JSON document with the media metadata, a
speaker-diarized transcript, segment and word time codes.

```
media file ──upload──▶ convodump server ──▶ ffprobe ─▶ ffmpeg (16 kHz mono) ─▶ nemo-speech ─▶ JSON
```

* ASR: [`nvidia/nemotron-3.5-asr-streaming-0.6b`](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b)
* Diarization: [`nvidia/Nemotron-3-Diarization`](https://huggingface.co/nvidia/Nemotron-3-Diarization)
* Engine: [NeMo-Speech.cpp](https://github.com/NVIDIA/NeMo-Speech.cpp) 0.3.0 (`nemo-speech`): Metal on macOS, CUDA on Linux, same output on both.

There is exactly one inference path: `convodump server run` does the work, and every machine
(the server's own included) uses `convodump media run <file>`, which uploads to the configured
server. Switching servers is a config change; there is no automatic failover.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/home-still/convodump/main/docs/install.sh | sh
```

Supported: macOS arm64 and Linux x86_64. The binary lands in `~/.local/bin/convodump`.

### The engine (server machines only)

```sh
curl -fsSL https://github.com/NVIDIA/NeMo-Speech.cpp/raw/main/scripts/install.sh | sh -s -- --version 0.3.0
nemo-speech pull nvidia/nemotron-3.5-asr-streaming-0.6b
nemo-speech pull nvidia/Nemotron-3-Diarization
```

`ffmpeg` and `ffprobe` must be installed as well.

## Use

```sh
convodump media run talk.m4a                       # speaker turns, one per line
convodump media run talk.m4a --output json         # the full document, one compact line
convodump media run a.mp4 b.wav --out-dir out      # out/a.convodump.json, out/b.convodump.json
convodump server status                            # health of the configured server
```

```
talk.m4a  duration 00:00:07.000  speakers 2  language en
[00:00:00.000 → 00:00:01.400] S1: alpha bravo charlie
[00:00:05.000 → 00:00:05.900] S2: echo foxtrot
```

Exit codes: `0` success, `1` general error, `2` usage or configuration error, `3` server
unreachable, `4` partial success (some files failed).

## Output schema (`schema_version` 1)

```json
{
  "schema_version": 1,
  "convodump_version": "0.1.0",
  "source": { "file_name": "talk.m4a", "size_bytes": 123456, "sha256": "…" },
  "media": {
    "format_name": "mov,mp4,m4a,3gp,3g2,mj2", "format_long_name": "QuickTime / MOV",
    "duration_s": 7.0, "size_bytes": 123456, "bit_rate": 80000,
    "tags": { "title": "Sample" },
    "streams": [ { "index": 0, "codec_type": "audio", "…": "raw ffprobe stream" } ],
    "chapters": [],
    "transcribed_stream_index": 0
  },
  "engine": { "name": "nemo-speech", "version": "…", "asr_model": "…", "diarization_model": "…", "language_requested": "auto" },
  "transcript": {
    "text": "alpha bravo charlie echo foxtrot",
    "languages": ["en"], "duration_s": 7.0, "confidence": 0.93,
    "speakers": [ { "id": "S1", "word_count": 3, "segment_count": 1, "talk_time_s": 1.2, "first_start_s": 0.0 } ],
    "segments": [ { "index": 0, "speaker": "S1", "start_s": 0.0, "end_s": 1.4, "start_tc": "00:00:00.000", "end_tc": "00:00:01.400",
                    "text": "alpha bravo charlie", "first_word": 0, "last_word": 2 } ],
    "words": [ { "index": 0, "text": "alpha", "start_s": 0.0, "end_s": 0.4, "confidence": 0.99, "speaker": "S1" } ],
    "unattributed_word_count": 0
  },
  "timing": { "probe_ms": 40, "decode_ms": 300, "engine_ms": 2100, "total_ms": 2450 }
}
```

* Speaker ids (`S1`, `S2`, …) are anonymous labels; the tool never names anyone.
* A new segment starts when the speaker changes or the same speaker pauses for more than 2 s.
* `format.filename` from ffprobe is dropped: it is the server's temp path.
* A transcript with text but no word timings is an error, not a degraded result.

## Configuration

`~/.home-still/convodump/config.yaml` (or `$HOME_STILL_CONFIG_DIR/convodump/config.yaml`), then
`HOME_STILL_CONVODUMP_<SECTION>__<KEY>` environment variables, then CLI flags. Every key and its
default is in [`config/default.yaml`](config/default.yaml). Unknown keys are an error.

```yaml
client:
  server_url: http://127.0.0.1:7450
  token: "<token>"
server:
  bind: 127.0.0.1:7450
  token: "<token>"
```

The server refuses to start without `server.token`; clients refuse to upload without
`client.token`.

### GPU schedulers

`engine.wrapper` is an argv prefix placed before the `nemo-speech` command. A host that
schedules its GPU through a lease command sets it to that command, so every job runs
under the lease:

```yaml
engine:
  wrapper: ["<scheduler>", "run", "--", ]
```

## HTTP API

| Method | Path | Notes |
| --- | --- | --- |
| `GET` | `/v1/health` | No auth. `{status, convodump_version, engine{…}, busy}` |
| `POST` | `/v1/media` | `Authorization: Bearer <server.token>`; multipart `file` (required), `language` (optional). Returns the document. |

Errors: `{"error": {"code", "message", "cause", "fix"}}` with `400 missing_file`,
`401 unauthorized`, `413 too_large`, `422 no_audio_stream` / `unreadable_media`,
`502 engine_failed`, `500 internal`. One job runs at a time; later requests queue.

## Run the server as a service

* macOS: render `deploy/com.home-still.convodump.plist` (replace `@HOME@`), put it in
  `~/Library/LaunchAgents/`, then `launchctl bootstrap gui/$(id -u) <plist>`.
* Linux: copy `deploy/convodump.service` to `~/.config/systemd/user/`, then
  `systemctl --user enable --now convodump`.

## Privacy

* The server keeps nothing: each job lives in a temp directory that is deleted when the job
  ends, on success or failure. Logs hold the job id, size, timings and outcome only, never a
  file name or any transcript text.
* The repo is guarded by [`scripts/privacy-scan.sh`](scripts/privacy-scan.sh), run by the
  `pre-commit` and `pre-push` hooks (`git config core.hooksPath .githooks`) and by CI. It
  rejects media files, private IP addresses, home-directory paths, email addresses (other than
  GitHub no-reply), large files, secrets (gitleaks) and anything on a local denylist kept
  outside the repo at `~/.config/convodump/privacy-denylist.txt` (one case-insensitive string
  per line).

## Licenses

ConvoDump is MIT-licensed. Both models are released under
[OpenMDW-1.1](https://openmdw.ai/license/1-1/); see the model cards linked above.
