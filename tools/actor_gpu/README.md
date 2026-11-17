# Actor GPU slot — root review required

Eight step-mode arms, candidate SELF from READY.json. No console contact by the implementer.
Both new keys default off. `actor_discard_free_shader` targets uncut environment-material actors; final strict proof is in host-proof.json; `actor_frontface_cull`
and combined are **PICTURE-CHANGING, NOT APPROVED FOR RELEASE**. Native timing and picture
are unmeasured. Keep render_scale 0.75. The latter removes GL front-facing triangles,
including open/double-sided surfaces: inspect screenshots before granting the experiment slot.

## Arm order

wchess1 owner 796..1276: control, discard-free, frontface, combined.
centipede1 376..856: control, discard-free, frontface, combined.
Each replay runs its full tape. Each has exactly 481 logged frames. Analyze only against
that map's control. EndScene on frame N+1 waits for frame N GPU work; inspect rows
850/962/1047/1048 and the full playing window. Host counts do not predict Vita milliseconds.

## Start a fresh root session

These are instructions for root, never executed by the implementer. No Terminal.app needed.
READY.json pins the kit, SELF, configs and tapes. Review REPORT.md, host-proof.json and
screenshots first. Confirm coordinator slot, AC and the root FTP lock using the established
protocol. Preserve the true owner SELF, both configs and saves; never back up a test SELF.

```bash
P=/Users/anezium/vita-j0/plan-2026-11-12/actor-gpu
KIT=/Users/anezium/vita-j0/plan-2026-11-12/rc1-session/rc1kit.py
export RC1_SESSION="$P"
python3 "$KIT" --session "$P" preflight
python3 "$KIT" --session "$P" log start arms
python3 "$KIT" --session "$P" observe
# Inspect the printed PNG, then use its exact path:
python3 "$KIT" --session "$P" closed --observed /absolute/printed.png
python3 "$KIT" --session "$P" backup
python3 "$P/verify-session.py" backup
python3 "$KIT" --session "$P" tape get rw-20261116-wchess1-01.lgtape
python3 "$P/verify-session.py" tape rw-20261116-wchess1-01.lgtape
python3 "$KIT" --session "$P" tape get fp-centipede1-01.lgtape
python3 "$P/verify-session.py" tape fp-centipede1-01.lgtape
```

`tapes/` is intentionally empty before root retrieves the device copies. If absent,
put exactly ONE tape from READY's pinned source, observe, inspect, acknowledge, then get
and verify it. Repeat the complete transaction separately for the second tape. No two-tape
put transaction. No driver state, backups or logs are copied from gpu-ablation-2.

Install READY's `self_path` and `console-configs/wchess1-control.json` using the kit's
`install --eboot ... --config ...` under a current acknowledged observation. Then:

```bash
python3 "$P/actor-gpu-step.py" --step
# Inspect the exact printed PNG, then acknowledge it:
python3 "$KIT" --session "$P" closed --observed /absolute/printed.png
# Repeat --step and the separate inspect/ack for each remaining arm.
```

The driver swaps the next config only after the latest acknowledged observation. It
refuses any stale/failed marker, wrong SELF/config/tape, replay mismatch, dropped or
missing/nonfinite rows, killed quit, missing final done or foreign transaction.
It never acknowledges, retries or restores automatically. A failure leaves a durable
`actor-gpu-step-active ... FAILED` marker; stop and use root recovery. Do not delete it
merely to retry, and do not credit an incomplete arm.

## Close the slot

```bash
python3 "$P/analyze.py"
python3 "$KIT" --session "$P" restore --keep-saves
# Verify owner/config read-back and unchanged saves, then:
python3 "$KIT" --session "$P" log stop
python3 "$P/verify-session.py" shutdown
```

Retain restore.json, shutdown receipt, stopped receiver/process evidence and final observed
LiveArea. Preserve foreign jobs. No task Terminal window exists. `analysis.json` reports each
map's p50/p95/max, GPU wait, scene share and rows 850/962/1047/1048; centipede1 includes
only row 850 because the other named frames lie outside its selected window. A single
control does not bound run-to-run noise: small gains need a repeat, never a default flip.

## Offline checks

`python3 test-step.py` exercises all eight arms with `tests/fake-kit.py`, plus failure
latches and one real-kit parser compatibility test. No device transport is loaded for execution.
`python3 test-oracle.py` tests payload/event/pixel mutants. Production host and native builds
have revision/hash receipts. `seal.py` refuses missing proofs or a modified kit.

## Picture review

Open SCREENSHOTS.md for recorded-input replay before/after pairs and supplemental close
views of the bishop and army ant. Supplemental views use a fixed render camera and an
intentional bounded stop, not a replay PASS; all their sampled simulation states match their
control. Their host window is 1200x648; tape-oracle views are 960x544. Scale stays .75 in
both. Never infer the Vita's pixel cost from either host window. GL_BACK was rejected and
is absent from the arm plan. GL_FRONT still changes pixels and has no owner release approval.
