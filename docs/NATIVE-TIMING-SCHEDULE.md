# Fixed native capture window

The optional `schedule` startup configuration fixes one window at `S = ready_us + ready_offset_us`, `T = S + 30,000,000us`. The offset is declared before launch (20–600 seconds), the same for both comparison arms. A different map, another installation/generation, late review or clock fault rejects this capture. It never moves the window or changes gameplay, the simulation clock, pose, saves or progression. Defaults retain the earlier passive acquisition path.

```json
{"enabled":true,"audio_raw_counters":true,"audio_producer_capture":true,"schedule":{"map":"pandemonium","ready_offset_us":180000000}}
```

Association hashes remain deployment-supplied/UNKNOWN unless independently recorded. The map above is an example, not permission to load or manufacture a save. Ordinary loading and resume remain physical/user actions.

Before the review deadline, an overlay identifies the run and a submitted viewer frame: token ordinal, viewer sequence, native timestamp, generation and operation. Every submitted viewer frame receives a unique reference with a128-bit HMAC-SHA256 tag. The32-byte key is generated once at startup and never exported. A full32-byte standard HMAC known-answer test runs at startup on the actual backend; failure rejects the schedule. A bounded128-token ring retains recent references; an older issued reference is authenticated without retaining an unbounded frame history. Pair state comes from actual window-boundary/frame records, not a stale anchor. This identifies submitted rendering; physical camera exposure time remains UNKNOWN. Token submission must be no more than60seconds before native acknowledgement.

Use Terminal capture. The image-only tool must RETURN, the model must inspect that actual image, then a SEPARATE action prepares/uploads `native-timing-review.json` in the normal preferences directory. `tools/native_timing_review_request.py` writes an exclusive local request; it does not upload, view an image or approve anything. Upload the request to a separately recorded initially-absent temporary filename, verify its full readback, then atomically rename it to `native-timing-review.json` before the deadline. This prevents the game consuming a partially uploaded file. Both temporary paths belong to the fresh original-unit absence/restoration inventory; no general cleanup is authorized. Close FTP immediately, then keep all root ports quiet through the scheduled window. Copy the run ID and the five reference fields and authentication tag from the same inspected frame. No `approved` field or filename is evidence of inspection. Native consumption must finish by `S-10seconds`; it authenticates the exact issued token, run, generation and operation. Missing, malformed, oversized, stale or mismatched requests reject without sliding S.

The request file is polled at most twice per second ONLY before that deadline, using a bounded1025-byte read. Tokens and file IO cease before the ten-second settle/window. The host must retain separate image-return/inspection/action chronology and quiet-port evidence; the native receipt does not attest external FTP, camera or driver requests.

Frame and producer acquisition start automatically around `S-12seconds`, retaining the ten-second settle period and complete guards. The producer seals automatically at the first real viewer begin at/after `T+2seconds`; more than one second additional lateness rejects. Whole viewer intervals and whole producer blocks/fences are retained, with overhangs identified by the existing strict checker. A modal entry immediately requests producer sealing before the blocking menu loop; input, non-gameplay, changed generation or observed save/capture/console attempts invalidate scheduled acquisition. The capture does not wait for a later human Quit. Quit normally after the CLOSED overlay; no automatic exit or synthetic gameplay action is introduced.

The diagnostic journal keeps its existing capacities:6144frames,8192blocks,128fences and128legacy reports. Saturation/drop still rejects; there is no enlarged buffer or fabricated zero. Startup allocation adds only fixed scheduler metadata and the bounded token ring. The exporter still streams through its32KiB buffer. Requested storage and the64KiB export allowance require the actual host probe; these are not native RAM, allocator overhead or stack-high-water measurements.

`native_timing_check.py` checks optional scheduler receipts, the global (uncompressed) viewer sequence, fixed residence age, native acknowledgement deadline, ring identity, complete guard coverage and automatic seal. It continues to reject record/producer loss and to report camera exposure, external quiet, route activity, provenance and simulation-age equivalence as unqualified/UNKNOWN. Submitted tokens and a single successful capture do not establish a matched pair,13-map campaign, hearing, p95, raw-zero, heap or game performance acceptance. Predeclared paired-state tolerances and a separate external evidence join remain required.

The v2 token message is exactly 48 bytes: `LGNTW001` followed by five little-endian u64 fields (ordinal, viewer sequence, submitted microseconds, generation, operation). Its tag is the first 16 bytes of HMAC-SHA256. Python checks length, unique retained frame references/tags, issued ordinal bounds, retained/reviewed consistency, generation, operation, maximum age and the deadline. Python cannot verify HMAC because the per-process key is withheld; a structurally consistent receipt remains UNATTESTED, including an evicted reviewed token. The native ACK path performs authentication.

The host memory proof inventories every production `native_timing` static and rejects unaccounted additions. `SESSION_CLAIM`, the producer `UNIVERSE`, and the host-only clock `ORIGIN` are already included in the producer bootstrap reservation. The two raw counter slots and the operation enabled/epoch/sticky-wrap statics (1 + 4 + 1 = 6 bytes, checked against their actual Rust sizes) consume auxiliary reserve. Probe allocator and test-only statics are excluded from production accounting. These are requested host payload and measured host export-thread stack numbers; linker padding, allocator/OS overhead, native RAM and native stack high-water remain unmeasured.

V3 changes only the pre-deadline review readout. Four white bitmap lines on an
opaque 960x120 black box replace the small review font. Each bitmap pixel is a
3x3 square; the glyph is 4x7 pixels (12x21 screen pixels), with a 13-pixel advance
and a 30-pixel line pitch. The slashed zero differs from 6 and 8; lowercase b/d
and c/e have distinct bitmaps. The canonical table and rectangle geometry live
in `src/native_timing/overlay.rs`; the Python renderer/reader reads that Rust
table directly. No font file or new Rust dependency is loaded. The existing
post-capture CLOSED notice remains outside this review readout.

Every value is lowercase hexadecimal in groups of four, including numeric
fields (fixed 16 hex digits for a u64). The four lines are:

```text
run=RRRR RRRR RRRR RRRR RRRR RRRR RRRR RRRR ck=XXXX
auth=AAAA AAAA AAAA AAAA AAAA AAAA AAAA AAAA ck=XXXX
t=TTTT TTTT TTTT TTTT|f=FFFF FFFF FFFF FFFF|u=UUUU UUUU UUUU UUUU ck=XXXX
g=GGGG GGGG GGGG GGGG|o=OOOO OOOO OOOO OOOO|s=SSSS SSSS SSSS SSSS ck=XXXX
```

`t` is the token ordinal, `f` the submitted viewer sequence, `u` the submitted
native microseconds, `g` generation, `o` operation and `s` the fixed start S.
The longest line is 73 characters and fits in 953x120 pixels including margins.
For each line, CRC-16/CCITT-FALSE uses poly 0x1021, init 0xffff, no reflection,
no final XOR, and the ASCII bytes before ` ck=` with every group space removed.
There are no other spaces in the body: field separators are literal `|`.
There is no newline, NUL, ` ck=` or check code in the CRC input. Thus the exact
byte-string forms are `run=` + 32 hex digits; `auth=` + 32 hex digits;
`t=` + 16 hex digits + `|f=` + 16 hex digits + `|u=` + 16 hex digits;
and `g=` + 16 hex digits + `|o=` + 16 hex digits + `|s=` + 16 hex digits.
For example `run=2060 52db 4ecc 5124 280c 7235 1533 8d28` checks the exact bytes
`b"run=206052db4ecc5124280c723515338d28"`. The standard `123456789` vector is
0x29b1. CRCs detect transcription errors; they do not authenticate any data.

After returning and visually inspecting the actual image, pass all four lines
(including checks) to `tools/native_timing_review_request.py --lines-file FILE
--output REQUEST` or repeat `--line '...'` four times. Alternatively supply
`--run-id`, the five **decimal** `--token` fields, `--auth`, decimal `--start-us`,
and all four copied `--checks`. All CRCs are mandatory in either mode. The helper
names a bad line and refuses to create the request on mismatch. Never regenerate
checks from guessed data to bypass a mismatch. The ACK JSON, HMAC input, token
ring, S/T, deadline, clocks and acquisition buffers are unchanged.

`tools/native_timing_overlay_read.py CAPTURE.png` reads 1280x720 driver images,
locates the top bitmap grid/opaque box, decodes its glyphs and verifies all four
CRCs. Run it with the existing `/Users/anezium/vita-j0/venv/bin/python` (Pillow).
Its JSON contains four transcribed lines and a prominent transcription-aid label;
it never writes a review request. It refuses missing, ambiguous or corrupt text.
The reviewer must still look at the actual image; a valid CRC or synthetic-reader
test is no evidence of human inspection, native HMAC acceptance or camera timing.
The reader currently supports the full-screen driver scale with a small positive
capture offset; arbitrary perspective, crop, rotation and exposure are untested.
The glyph static payload is charged in the host auxiliary memory proof.

The host-only matched-pair CLI, frozen 150-second Gate 1 template, observed-state
brackets, frame metrics and independent evidence receipt contract are documented
in [NATIVE-TIMING-PAIR.md](NATIVE-TIMING-PAIR.md). A successful single export or
pair assessment still leaves method/campaign qualification false.
