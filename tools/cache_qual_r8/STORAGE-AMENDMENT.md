# Recorder storage clarification before console data

This separate file supplements the initial no-amendments marker. The sealed
CONSOLE-BRIEF.md remains byte-identical. No console or shot driver was contacted.

ARM disassembly of the initial native build proves a 1,245,184-byte reservation
for each 8,192-row window (152 bytes/row). Six windows therefore reserve 7.125 MiB
per launch in BOTH arms, plus small window/event metadata. This storage remains
live through the final chapter change so every window and reset can be exported.
It is diagnostic recorder memory, distinct from the approximately 31 KiB cache.
Paired window/transition/load heap gates still require B-A <=+1 MiB and sampled
heap <=180 MiB. Report absolute heap as well as the paired difference.

The final engine streams borrowed typed rows directly to its buffered JSON file,
instead of first materializing a second JSON tree. This avoids a large transient
export allocation. Encoding still occurs only after stop, and all original
exports, absences, saves and the human SELF are restored exactly as specified.
The final linked binary is rebuilt and its native reservation is reverified.

Heap samples exceeding 180 MiB and any sampled self-check mismatch additionally
emit a diagnostic error for the existing passive stop monitor. These diagnostics
are opt-in; default cache/recorder behavior remains off. No reading rule changes.
Both final loose-helper and ZIP main-path offline rehearsals are mandatory.
