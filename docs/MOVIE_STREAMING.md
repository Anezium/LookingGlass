# Story movie streaming

`vita-stream` starts at `vita-input` (`bd653d5`). The opening and ending use
one implementation on every platform; the Vita-only movie skip is removed.

`Assets::mount` mounts the packs. `Assets::open` resolves the same normalized
paths and pack overrides as `read`, and returns an owned `Read + Send` stream.
zip 0.6.6's `ZipFile` erases `Send` on its borrowed reader, so a worker opens
an independent archive and reads its entry with `by_name`. A channel with one
queued 32 KiB chunk bounds read-ahead; dropping the reader disconnects and joins
the worker. ZIP errors, including CRC failures, reach the reader. There is no
seek adapter, temporary file, new dependency or platform-specific path.

`Video::new` scans a reader to validate the header, dimensions and chunk bounds
and count frames. Payloads are discarded in this pass. The caller reopens the
entry and passes the new reader to `Video::next` for sequential decoding. The
video retains one reusable chunk buffer, the small codebooks and two RGBA
frames. The existing limits remain: 128 MiB per asset/movie, 16 MiB per chunk,
2048 pixels per dimension and ten minutes of frames. Playback timing, pause,
skip and texture updates use the existing controls.

The movie MP3 remains in a `Cursor<Vec<u8>>`: rodio 0.20.1 requires
`Read + Seek + Send + Sync + 'static`. The shared soundtrack loader logs
`movie soundtrack <name>: <n> bytes in memory`; above 16 MiB it warns without
adding a rejection. Other audio loaders are unchanged.

## PC regression check

Use your existing game data folder; no original assets belong in Git:

```sh
mkdir -p private
cargo test --locked movie::tests
cargo test --locked assets::tests
cargo run --locked -- --data /path/to/Alice1/bin/base --movie-check
```

`--movie-check` decodes every frame and both complete soundtracks without
opening a window or an output audio device. It checks the decoded frame count
and the audio/video duration and writes sampled frames to ignored `private/`.
The unit tests cover short non-seekable reads, truncation, invalid motion,
case-insensitive override resolution, independent reopening, CRC failures and
dropping an active stream early.

Validated on macOS arm64 (2026-10-11): three movie tests and two asset tests
passed. The headless check decoded 3,731 opening frames and 1,952 ending frames,
both 512×256 at 30 fps, with soundtrack durations of 131.92 s and 65.04 s
including padding. The soundtrack logs reported 1,908,527 bytes and 1,040,718
bytes respectively, both below the warning threshold. No console was contacted.
