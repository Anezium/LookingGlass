# Static collision SAH r8

Mac implementation and package helpers. No game assets are included.
`collision_sah` defaults false. See CONTRACT.md for exactness and
READING-RULES.md for the fixed experiment and memory gates. The sealed output's
AMENDMENTS.md supersedes the unchanged inherited console brief.

Host target: /Users/anezium/vita-j0/cargo-target-host. Offline locked builds only.
`run_host.py OUTPUT` records four real 180-frame audio/player/NPC/camera/other
corpora, runs A/B simulation hashes and compares final captures. The ignored
`sah_real_maps_corpus_masks_and_load` test replays all corpora, checks every
observed content mask plus canonical masks and reports load/heap counters.

`prepare.py OUTPUT` only materializes local helper/config/protocol files.
`build_native.py OUTPUT`, `seal.py OUTPUT` and `finalize.py OUTPUT` build and
seal one common SELF. Run the mandatory `test_session_e2e.py OUTPUT
--include-optional --output OUTSIDE_PACKAGE.json` before fresh session creation.
It runs a separate two-launch smoke session through restoration, then a fresh
ten-launch wall/census session, with all network/process boundaries blocked.
Console execution requires its independent human/console owner; do not execute
it from the implementation worker.
