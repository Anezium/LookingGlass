from pathlib import Path
import json,hashlib,subprocess
P=Path(__file__).resolve().parent
sha=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
a=json.loads((P/'attribution.json').read_text())
text='''# Actor GPU candidates — root review and device slot required

No release defaults changed. No console, FTP, ports 1337/1338, receiver or Terminal.app
contact by the implementer. The rc1 kit is unchanged. Native speed and native picture
are **UNMEASURED**. This is a host-proven candidate package, not a release qualification.

## Measured problem and boundary

The supplied native ablation evidence isolates retained actors: wchess1 owner 796..1276
p95 80.8 -> 53.3 ms and frames 850/962/1047 114/126/117 -> 41/46/39 ms with actors
masked; centipede1 live-actor omission takes median 61 -> 40 ms and mean GPU wait
23 -> 13 ms. The mask changes pixels and remains diagnostic only. EndScene waits for
previous-frame GPU work: the requested draw steps are 849, 961 and 1046.

Fresh host attribution at those exact steps confirms 47 actor draws, 33,477 submitted
indices and 9,146 summed packet vertices per frame. Chess body surfaces carry **19
draws / 26,664 indices / 7,269 packet vertices**; counting their attachments gives
26 draws / 27,882 indices / 7,523 packet vertices. The remaining 21 actor draws carry
5,595 indices. These are submitted index counts, not unique transformed vertex counts;
packet vertices count repeated layer payloads. See attribution.json for every draw,
owner invocation, bounds, state and sample query.

**Host/native limitation:** RC15's host replay already diverges from these native tapes.
The strict oracle requires the candidate to reproduce that base host state and ordered
events exactly; it does not call the host replay a native PASS. On the owner tape the
host camera is still near the starting courtyard at the requested steps. Therefore
host coverage numbers below are not coverage measurements of the Vita's lesson spikes.
No native per-draw timing or native screen coverage is claimed.

## Per piece at steps 849, 961 and 1046

Each active saved.cast entry invokes Puppet once. Shared Puppet model identity is not
piece identity: the census joins identity **and invocation ordinal**, then names/feet.
The rows below are identical in draw/geometry count at all three steps.

| Piece (feet distinguish unnamed entries) | Body draws | Attachments | Total indices | Summed packet vertices |
|---|---:|---:|---:|---:|
'''
for piece in a['rw-20261116-wchess1-01.lgtape']['frames']['849']['pieces']:
 name=piece['piece']['name'] or piece['piece']['model']+' '+str(piece['piece']['feet'])
 text+=f"| {name} | {piece['body_draws']} | {piece['attachment_draws']} | {piece['indices']:,} | {piece['packet_vertices']:,} |\n"
text+='''
The second unnamed rook is active and invoked but its two body layers are removed by
the already-enabled packet frustum cull. Raw-group vs culled-group evidence preserves
that distinction. Eight other piece invocations reach submission. No repeated invocation,
afterimage, outline or shadow draw was observed for these pieces at the sampled steps.
The nine pre-cull bodies would have 21 layers; the existing cull leaves 19. No new duplicate
or invisible-draw removal is claimed.

## What one draw does

- Skinning runs on CPU workers/serial fallback and publishes world-space vertices. The GPU
  vertex program transforms each packet position by Model and Projection, passes UV, packed
  colour, normal/lightmap coordinates and world position. This is not GPU skeletal skinning.
- Pawn/rook/knight/bishop main surfaces have two authored stages: opaque `reflights.tga`
  sampled with environment UVs, followed by the character skin using source-alpha / one-minus-
  source-alpha blend. Pawn faces add a small default cutout surface; knights add a sword and
  shield, bishop a staff. Environment UV reflection and normal-dependent alpha are computed
  in layer_passes/Stage::view_uv **on CPU**; the GPU samples an ordinary 2-D texture. This is
  not a reflection render target and not a second whole-scene pass. Deferred/raw is a CPU
  preparation route, not an additional GPU rendering of the same published skin.
- Every observed chess body draw tests depth, uses LessOrEqual (depth_equal=false), and writes
  depth because the enclosing group is opaque. Even its blended skin layer writes depth under
  the current group policy. Layers retain authored order; opaque groups are submitted in group
  order, not sorted front-to-back. Transparent actor/FX groups share the world-alpha sort and
  use a read-only depth mask. No depth-test-off chess draw was observed. The pipelines default
  to CullFace::Nothing: both triangle windings can rasterize.
- The full fragment shader samples diffuse, computes alpha, contains four possible discard
  statements (dissolve, ghost, two cutout blocks), lighting/gain, dynamic lights, appearance tint,
  filter fade and fog. The shared source has eight conditional light contributions; native light
  selection caps at two. Compiler instruction count is unknown. The model branch excludes
  lightmap atlas sampling; optional sky/fog paths still exist in the shared source.
- Source camera_portal::prepare_camera_portals draws world batches plus dedicated pupil/cat
  portal actors. It does not iterate wchess1 saved.cast. set_reflection controls brush poses,
  not another pass of these chess pieces. All queried actor draws target the same 720x408 3-D
  viewport at scale .75. There is no per-piece target switch. finish_world begins the existing
  3-D target once around actor/world-alpha submission; presentation later upscales to 960x544.
  The host query's masked no-depth duplicate is diagnostic instrumentation only, never a
  production pass or evidence of production double draws.

### Host coverage and overdraw

At 849/961/1046 the chess-body post-depth samples are 100/101/101; masked no-depth
raster samples are 1,881/1,882/1,886. Values sum all layers and overlapping triangles,
not unique covered pixels. Thus the host replay mostly draws distant/occluded pieces;
it does not establish that the native lesson spikes are fill-bound. Actual host depth
rejection is large. Bounds and projected long-side values for every packet are in the
attribution; these are conservative boxes, not visible area.

At centipede1 host steps 500/600/700/800 there are 7 ant-labelled body draws each,
16,959 indices / 5,168 packet vertices, all tested and depth-writing; this
is six army-ant bodies (2,472 indices / 717 vertices each) plus one antlion
(2,127 / 866), not seven identical enemies. Each army ant also submits one musket
(453 indices / 96 vertices), so its complete invocation has two draws, 2,925
indices and 813 packet vertices. The six army ants including muskets total
12 draws / 17,550 indices / 4,878 packet vertices; their queried
post-depth samples are zero in this divergent host view. Total retained actors are
46/46/38/38 draws, 35,106/35,106/32,622/32,622 indices. Body surfaces, attachments
and effects are not interchangeable populations. Native ablation, rather than these
host coverage samples, establishes that the ants carry native GPU load.

## Candidates

1. **Exact: actor_discard_free_shader=true (default false).** Uses the existing shader
   with only the four discard statement lines removed. The selector requires a non-effect
   packet, no dissolve/ghost, and **Cutout=0 on an environment-material actor surface only**. The broad texture-opacity certificate route was rejected
   by the potears1 strict pixel oracle (85/1200 changed images), and Cutout=0 alone
   still failed there. The final scope excludes ordinary uncut actors such as ladybugs
   and requires an authored environment stage on the same surface. These failed routes
   are excluded, not relabeled exact.
   It remains unchanged for the older independently selected corpse/opaque keys. Blending, depth equality and depth-write do not change
   whether discard can occur; the new selector therefore covers blended actor layers too.
   Complete geometry, order, texture, uniform words, colour/fog/lighting arithmetic and
   pipeline depth/blend state remain unchanged. Failed compilation falls back to the original
   program. Texture certificates are weak-handle/generation checked and do not keep assets alive.
   Host steps 849/961/1046 qualify the 16 main chess-body layers; 18 of 47 actor draws
   qualify under the final uncut environment-surface selector. Default cutout pawn-face/other body layers
   remain on their original route. This exact candidate does not qualify the six army-ant
   default-cutout body surfaces, so a direct centipede1 body gain is not predicted.
   The potential GPU benefit is avoiding unreachable fragment rejection / enabling earlier depth
   work. **That hardware mechanism and its millisecond benefit remain assumptions until the slot.**
2. **PICTURE-CHANGING: actor_frontface_cull=true (default false).** Cull GL_FRONT for
   non-effect actor packets only, restore cull-disabled state immediately after each draw.
   Same vertices/indices/shader/depth/blend, fewer rasterized faces. It may remove authored
   double-sided/open surfaces; there is no claim that all models share a safe winding.
   Owner approval is required before release. Combined enables this and discard-free.
3. **REJECTED: earlier GL_BACK experiment at cd12358.** Host screenshots visibly remove
   Alice's exterior faces and expose the opposite side. It changed 6,249/6,375 owner images.
   It is excluded from final arms. Evidence remains under host/*-backface and builds/cd12358;
   the corrected experiment names the literal GL_FRONT operation rather than asserting a
   universal authored back-face convention.

No lower render scale, mesh decimation, data edit, global dependency update or release
config promotion. The exact candidate deliberately leaves draw count unchanged; no claim
that its gain alone reaches p95 <=50 ms or saves >=8 ms is supported yet.

## Strict oracle

The clean RC15 control was rebuilt while the worktree was detached at 610ee98; its binary
SHA-256 exactly matches the original control build, ruling out build/source overlap.
Every frame compares full state, ordered events, complete CPU/GPU geometry, texture identity,
all uniform words and packet policy/order, world and joint ordering, selected lights,
presentation and pixel hash, with no tolerance. Unknown payload fields stay compared.

Normalization is limited to clocks, documented memory observers, volatile GPU slot identity,
two inherited dynamic-collider observer counters, and the explicitly named submission observers:
`vita_actor_discard_free_draws`, `vita_fx_uniform_bytes`, `vita_fx_pipeline_depth_sets_skipped`,
`vita_fx_state_applications`, `vita_fx_uniform_sets_skipped`. A different compiled program has
inactive discard uniforms and its own uniform/state cache, so these command-traffic observations
can legitimately differ. They are not draw omissions. Raw logs and original failed receipts are
retained. Draw/vertex/index counts, drops, semantic counters and complete uniform payloads are
not normalized. No shader/pixel difference is hidden by this normalization.

The first limited exact delivery was 05a1d205 (FIRST-EXACT.json, full a2-04).
The later full-suite run rejected the broad selector on potears1: packets/uniforms matched
but 85 images differed. This is a real retained failure, never waived by tolerance.
A second Cutout=0-only attempt also failed potears1. At the first broad mismatch
(464), only 13 pixels in rectangle [478,23,482,27] differed by at most 3 channel units,
but the zero-tolerance gate rejected it. Final scope keeps both certified-cutout and
ordinary non-environment actors on their original shader routes. Its new full-suite
results below supersede the partial first delivery; the exact reason for the broad shader
pixel differences is UNKNOWN. Compiler/precision or linked interpolation is a hypothesis,
not an established diagnosis. Final-runtime comparisons:

| Tape | Frames per pair | All new flags OFF | Discard-free exact | Specialized draws |
|---|---:|---|---|---:|
'''
proof=json.loads((P/'host-proof.json').read_text()) if (P/'host-proof.json').exists() else []
for tape in ['a2-04.lgtape','a4-wchess1-01.lgtape','rw-20261116-wchess1-01.lgtape','fp-centipede1-01.lgtape','fp-hatter2-01.lgtape','a4-skool1-01.lgtape','a4-potears1-01.lgtape']:
 rows=[r for r in proof if r['tape']==tape]
 states=['PASS' if any(r['candidate']==name and r['exact'] for r in rows) else 'PENDING' for name in ['env-off','env-discard-free']]
 frames=rows[0]['frames'] if rows else 'pending'
 hits=next((r['coverage']['vita_actor_discard_free_draws'] for r in rows if r['candidate']=='env-discard-free'),0)
 text+=f'| {tape} | {frames} | {states[0]} | {states[1]} | {hits:,} |\n'
text+='''
Native replay PASS is a separate slot gate. Host/base divergence is reproduced exactly,
not mislabeled as candidate failure or silently waived. Picture-changing arms separately
require exact full simulation state/events/replay but report changed image frames; see
picture-proof.json and screenshots.json. Images remain local game-data derivatives, never Git.

## Package and closure

READY.json is authoritative for SELF, driver, configs, kit and proof file hashes.
FINAL-VERIFICATION.json pins READY itself and REPORT without a circular manifest. The source
runtime/build head and final source head are recorded separately; package/docs-only commits do
not change production source. README gives one-tape transactions, separate observation/ack,
one arm per --step, recovery and final restore/read-back/shutdown. Root runs the slot.

Arm order: wchess1 796..1276 control -> discard-free -> frontface -> combined;
centipede1 376..856 control -> discard-free -> frontface -> combined. Eight whole-tape arms.
Every arm logs 481 inclusive rows; analyzer compares each map against its own control and
prints total/GPU-wait rows for 850/962/1047/1048 (only 850 lies in the centipede1 window).
All gpu masks and omission diagnostics remain off, world_gxm stays live.

Offline tests exercise the actual driver through a fake kit, all eight arms, malformed/tampered
tapes/configs, missing/nonfinite/dropped rows, bad replay, killed quit, duplicate final done,
foreign transaction and final-drained-log gates. One compatibility test uses only the real kit's
pure log/config readers. The kit hash must remain 93f434a6286b2180bd66d89ffad1394b7f27b5e385296c67a70b2271256558b3.

No host CPU timing run was made; mac-timing.lock was neither acquired nor modified. Counts,
exact replays and builds do not need it. Private build caches are removed only after packaged
artifacts, link/map/static proof and captured target-side link inputs are retained.
No standalone release source/relink bundle is assembled by this GPU experiment; the
VPK is a local test artifact for root, not a distribution candidate. See the closure receipt
for actual filesystem free bytes; the 41 GiB reserve is checked before large writes. No other
worker's cache or worktree is deleted. No task driver/receiver or Terminal window was opened;
there is no console restoration/window-count claim from this host-only task.
'''

native=json.loads((P/'builds/2f06058/BUILD-RECEIPT.json').read_text())
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd='/Users/anezium/vita-work/lg-actor-gpu',text=True).strip()
text+='\n### Final artifact identities\n\n'
text+='| Item | Identity |\n|---|---|\n'
for label,value in [('Runtime / native / host oracle head',native['head']),('Source branch head at report',head),('SELF SHA-256',native['self_sha256']),('ELF SHA-256',native['elf_sha256']),('VPK SHA-256',native['vpk_sha256']),('Driver SHA-256',sha(P/'actor-gpu-step.py')),('Kit SHA-256',sha(P.parent/'rc1-session/rc1kit.py'))]:
 text+=f'| {label} | `{value}` |\n'
if (P/'READY.json').exists():text+=f"| READY SHA-256 | `{sha(P/'READY.json')}` |\n"
text+='\nFinal native static gates: RX-to-RW margin 284,772 bytes; exact pinned library/map selection, notices and 27-member VPK/LiveArea checks PASS. Final Rust: 1422 passed, 16 ignored; feature-off check PASS. Fake-kit: 14 tests / 528 assertions PASS, eight arms end-to-end. Comparator: three mutation/normalization tests PASS.\n'
text+='\nThe final owner census uses the final runtime and is bounded through 1050; centipede1 through 802. census-proof.json checks every selected state/packet/pixel against the full clean control. Masked no-depth raster instrumentation failed image neutrality on centipede1 and was rejected; its replacement uses samples queries on production draws only, with no extra raster draw. Centipede1 raster coverage is unavailable, not zero. Rejected raw logs/receipt remain in census-final/*-rejected-raster and census-raster-rejection.json.\n'
text+='\nThe opt-in reuses the existing discard-free program when the RC15 corpse key already requests it; it prewarms 217 additional pipeline objects (all blend/equality states rather than opaque-only). Native heap cost is not measured. Both flags off preserve the control route. See SCREENSHOTS.md for all before/after pairs; supplemental fixed-camera captures are bounded diagnostics, not whole-tape results.\n'

(P/'REPORT.md').write_text(text)
print('report written',len(text))
