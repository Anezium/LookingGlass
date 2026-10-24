# Alice static acoustic cache r6

Source and reproducibility tools for the Mac-only implementation. Use prepare.py OUTPUT, run_tests.py OUTPUT, build_native.py OUTPUT, seal.py OUTPUT, then the local offline tests documented in CONSOLE-BRIEF.md. No cargo update. Assets are read from the external private game-data directory and never copied into this repository or the handoff ZIP.

The console protocol remains unexecuted. All scripts that can reach device/driver transport are handoff helpers, not preparation commands. test_session_e2e.py blocks network and process boundaries before importing them and supplies serialized in-memory transport.
