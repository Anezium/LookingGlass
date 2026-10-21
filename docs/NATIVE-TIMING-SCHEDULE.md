# Fixed native capture window

The optional `schedule` startup configuration fixes one window at `S = ready_us + ready_offset_us`, `T = S + 30,000,000us`. The offset is declared before launch (20–600 seconds), the same for both comparison arms. A different map, another installation/generation, late review or clock fault rejects this capture. It never moves the window or changes gameplay, the simulation clock, pose, saves or progression. Defaults retain the earlier passive acquisition path.

```json
{"enabled":true,"audio_raw_counters":true,"audio_producer_capture":true,"schedule":{"map":"pandemonium","ready_offset_us":180000000}}
```

Association hashes remain deployment-supplied/UNKNOWN unless independently recorded. The map above is an example, not permission to load or manufacture a save. Ordinary loading and resume remain physical/user actions.

Before the review deadline, an overlay identifies the run and a submitted viewer frame: token ordinal, viewer sequence, native timestamp, generation and operation. A bounded128-token ring retains actual state for those frames. This identifies submitted rendering; physical camera exposure time remains UNKNOWN. Token submission must be no more than60seconds before native acknowledgement.

Use Terminal capture. The image-only tool must RETURN, the model must inspect that actual image, then a SEPARATE action prepares/uploads `native-timing-review.json` in the normal preferences directory. `tools/native_timing_review_request.py` writes an exclusive local request; it does not upload, view an image or approve anything. Copy the run ID and the five reference fields from the inspected overlay. No `approved` field or filename is evidence of inspection. Native consumption must finish by `S-10seconds`; it validates the exact retained token, run, generation and operation. Missing, malformed, oversized, stale or mismatched requests reject without sliding S.

The request file is polled at most twice per second ONLY before that deadline, using a bounded1025-byte read. Tokens and file IO cease before the ten-second settle/window. The host must retain separate image-return/inspection/action chronology and quiet-port evidence; the native receipt does not attest external FTP, camera or driver requests.

Frame and producer acquisition start automatically around `S-12seconds`, retaining the ten-second settle period and complete guards. The producer seals automatically at the first real viewer begin at/after `T+2seconds`; more than one second additional lateness rejects. Whole viewer intervals and whole producer blocks/fences are retained, with overhangs identified by the existing strict checker. A modal entry immediately requests producer sealing before the blocking menu loop; input, non-gameplay, changed generation or observed save/capture/console attempts invalidate scheduled acquisition. The capture does not wait for a later human Quit. Quit normally after the CLOSED overlay; no automatic exit or synthetic gameplay action is introduced.

The diagnostic journal keeps its existing capacities:6144frames,8192blocks,128fences and128legacy reports. Saturation/drop still rejects; there is no enlarged buffer or fabricated zero. Startup allocation adds only fixed scheduler metadata and the bounded token ring. The exporter still streams through its32KiB buffer. Requested storage and the64KiB export allowance require the actual host probe; these are not native RAM, allocator overhead or stack-high-water measurements.

`native_timing_check.py` checks optional scheduler receipts, the global (uncompressed) viewer sequence, fixed residence age, native acknowledgement deadline, ring identity, complete guard coverage and automatic seal. It continues to reject record/producer loss and to report camera exposure, external quiet, route activity, provenance and simulation-age equivalence as unqualified/UNKNOWN. Submitted tokens and a single successful capture do not establish a matched pair,13-map campaign, hearing, p95, raw-zero, heap or game performance acceptance. Predeclared paired-state tolerances and a separate external evidence join remain required.
