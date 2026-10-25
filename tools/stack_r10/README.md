# r10 stack package tools

Host-only preparation for one SELF that can run all three measured switches. The console protocol is not executed from this tree.

Switches stay independent and default false: audio_static_cache, collision_sah, skin_actual_consumers. Wall A is all off. Wall B is all on. Census adds phase_census on both arms.

Session names, absent from the sealed ZIP: session-s10-stack-load-smoke and session-s10-stack.

prepare.py writes the package. build_native.py and seal.py produce the common SELF. test_session_e2e.py is the offline main path, including --main-admission. test_census_closure.py checks cache counters with the cache off and on.
