# Step 1: buffered ZIP directory parsing

The pinned ZIP parser remains authoritative. A 1 MiB read window turns its small central-directory reads into bulk reads. The explicit logical cursor answers per-record position queries without filesystem calls and keeps seeks within the window in memory. Streaming workers use the same reader.

Host proof: `ALICE_TEST_DATA=<original base folder> cargo test --manifest-path tests/pk3-host/Cargo.toml --locked -- --nocapture`. Every real-pack entry is compared with the unbuffered parser in archive order, including name, sizes, CRC32, local header, central header and data offsets. All override-resolved names and representative decoded payloads are compared. Game data stays outside this clone.

Hypothesis: the measured 51 seconds of central-directory parsing will fall substantially. Host timings cannot predict Vita storage timings. Indexing and corpus identity still use local headers until the next steps.
