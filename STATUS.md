# wchess1 render work status

Base: RC14 37f9d0694820a7b56cfeb30c89f978dcc6c61593.
Branch: vita-wchess1-render-20261116. Dedicated worktree lg-wchess1.
Package workspace: /Users/anezium/vita-j0/plan-2026-11-12/wchess1-render.

Two implemented default-off candidates:

- world_alpha_parameter_once: immutable parameter/model identity permits one
  generation/model/time check per alpha preparation pass, without changing uniforms.
- actor_skin_output_reuse: skip the CPU kernel for a completed exclusively owned
  publication slot from the immediately previous epoch when its numeric inputs,
  instance, CPU surface, UVs and publication mode are identical. Raw mode excluded.
  Reservation without successful completion is explicitly insufficient.

NOT READY. No native performance or exactness claim. At this checkpoint the shared
mac-timing lock is occupied by the active RC15 host oracle. Waiting is bounded,
without deleting another task's lock or interrupting its process. See the package
lock-wait.json and logs/run-host.log for the original deadline and progress.

Host reference executable reused only after checking its clean RC14 build receipt
and SHA-256 (7fa17e52f960a310837f35f5cdd04e9a290105202db72d8b5d6d900f9f91d824).
run_host.py performs attribution, fresh candidate build and strict full-tape proof
on a2-04, wchess1 A4, owner wchess1 play, skool1 and potears1 after acquiring the
shared lock. Coverage counters prove whether the candidates are exercised. Ordered
events, state, full skin/packet payloads, framebuffer hashes and presentation must
match base exactly. Native-recording divergence is compared against host base.

Offline checks completed: 12 real-kit orchestration tests (all 12 planned arms and
negative gates), 2 oracle comparator tests with payload/event/pixel mutants. Rust
suite, feature checks, host replays, attribution counts and native build are pending.
No READY.json is emitted until proof and build prerequisites pass.

Root package order: A4 700..1180, owner play 1486..1966, owner play 796..1276;
control / alpha-once / skin-reuse / combined in each window. All arms are explicit.
The driver revalidates replay_gates on the drained post-exit log, keeps its final
validated done line and asserts the captured observation transaction before credit.
Offline tests mock transports; no console, FTP, 1337/1338, Terminal.app, kit edits,
default flips or game assets in Git. Kit remains SHA-256 93f434a6286b2180bd66d89ffad1394b7f27b5e385296c67a70b2271256558b3.
