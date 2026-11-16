# wchess1 render package sources

These are source snapshots of the offline package in
`/Users/anezium/vita-j0/plan-2026-11-12/wchess1-render`. Copy scripts to that
external package directory before running them; their sibling paths refer to the
external package, not this source directory. Do not run preparation in this tree.

No tapes, game data, build artifacts or console state belong here. The kit is an
external pinned dependency and is never copied or modified. The external README
contains root's console procedure; these scripts grant no console authorization.

`run_host.py` acquires the shared lock with bounded 60-second waits, attributes
RC14, builds this worktree freshly using the shared host target, and performs the
strict full-tape oracle. Its `lock-wait.json` is task-local runtime state defining
the original bounded acquisition deadline. `prepare.py` writes explicit configs
and an offline test manifest. `build_vita.py` uses the RC14 native recipe and
checks disk/HEAD. `seal.py` refuses absent host/build/test evidence. See STATUS.md.
