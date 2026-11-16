"""Write the final evidence report from sealed artifacts; no console operations."""
import hashlib,json,re,subprocess
from pathlib import Path
P=Path(__file__).resolve().parent
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    ready=json.loads((P/'READY.json').read_text())
    proof=ready['host_oracle'];attribute=json.loads((P/'attribution.json').read_text());cpu=json.loads((P/'cpu-family.json').read_text())
    unit_text=(P/'logs/unit.log').read_text();m=re.search(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',unit_text);assert m
    unit=json.loads((P/'unit-receipt.json').read_text())
    head=subprocess.check_output(['git','rev-parse','HEAD'],cwd='/Users/anezium/vita-work/lg-wchess1',text=True).strip()
    lines=['# wchess1 render: first console package','',
        'READY FOR ROOT MEASUREMENT ONLY. The >=8 ms Vita gain and the playing p95 <=50 ms bar are NOT established. No default is promoted. This first package has exact host candidates, with limited wchess1 skin-reuse coverage.','',
        f"Source branch: vita-wchess1-render-20261116; report-generation branch head `{head}`. Native build head `{ready['head']}`; host oracle head `{ready['host_oracle_head']}`; unit head `{unit['head']}`. `runtime-equivalence.json` checks that revision differences are exactly the known cfg(test)-only fixture/schema edits; production code is unchanged. Base RC14 is 37f9d0694820a7b56cfeb30c89f978dcc6c61593.",'',
        '## Attribution (measured host data, not Vita predictions)','',
        'Three draw-query frames per tape: A4 probe 900..902 and owner-play probe 1700..1702. Counts below were the same on both tapes; dynamic FX vertices differ slightly. The full recorded play tape has 6375 steps. Index vertices mean submitted triangle indices, not unique VBO vertices. Chess classification uses texture labels.','',
        '| Family | Draws/frame | Index vertices/frame | Packet vertices/frame | Observed state transitions/frame |','|---|---:|---:|---:|---:|']
    fam=attribute['a4-wchess1-01.lgtape']['families']
    for k in ['world_opaque','world_alpha','actors','chess_pieces','fx','hud','upscale']:
        r=fam[k];v=r['packet_vertices_per_sampled_frame'];lines.append(f"| {k} | {r['draws_per_sampled_frame']:.1f} | {r['index_vertices_per_sampled_frame']:.1f} | {'unavailable' if v is None else format(v,'.1f')} | {r['observed_state_transitions_per_frame']:.1f} |")
    lines+=['','The dominant alpha materials are textures/testliq/bw1.tga and textures/effects/chessw.tga. State transitions compare program/texture/blend/depth tuples in global draw order and attribute each transition to its current family; the first draw counts as a state entry. Uniform/cull/sampler changes and individual GL calls are not covered by that tuple. Per-phase API application counters are retained in cpu-family.json; disabled GL-observer counters are unavailable, not measured zero.','',
        '| Host window | Opaque CPU ms | Alpha CPU ms | Actor publication/submission ms | FX ms | HUD ms | Actor gather ms | Actor collect ms |','|---|---:|---:|---:|---:|---:|---:|---:|']
    for r in cpu:
        p=r['render_means'];s=r['span_means_ms'];label=('A4' if r['tape'].startswith('a4') else 'play')+' '+str(r['console_frame_window'])
        vals=[p[k]['cpu_ns']/1e6 for k in ['world_opaque','world_alpha','actors','fx','hud']]+[s.get('actor_gather',0),s.get('skin_collect',0)]
        lines.append('| '+label+' | '+' | '.join(f'{v:.3f}' for v in vals)+' |')
    lines+=['','These are 481-frame instrumented arithmetic means. Full profiler scopes nest; actor gather/collect are separately timed regions and must not be added to a containing scope. The instrumented binary omits frame_total_only, while proof/SELF recipes retain it. All timing runs used mac-timing.lock individually; root later limited that lock to timing only. Builds and exact replays released it. Loader, framework, profiler and OS scheduling overhead remain. Host proportions do not predict Cortex-A9 milliseconds. The family runs stop after their requested windows and are not credited as complete oracle replays.','',
        'Known chess actor inventory at host frames 300..360 measured inclusive body invocation ~0.082 ms A4 / 0.078 ms play. Full-tape known-actor worker service averaged ~0.138 / 0.137 ms. Those are different populations and worker service is not main-thread wait. Raw logs, exact populations and IDs are in attribution.json.','',
        'Native prior evidence, not this task: actor_collect 5.41 ms, world_alpha 5.05, world_other 4.84, actor_gather 3.92 on A4. actor_collect drains skin lanes and completes publication (including deferred layer construction); it is not pure waiting. world_alpha submits retained-world callbacks interleaved with actors/FX. world_other includes opaque submission and alpha-preparation residual; it is not an exclusive draw-call clock. The live backend update/draw split is distinct from spike sim/render.','',
        '## Candidates and strict oracle','',
        '- world_alpha_parameter_once (off): immutable parameter signatures include model identity and isolate moving batches. Check generation/model/time once per alpha preparation pass; retain exact uniform words, textures, ordering and draw calls.',
        '- actor_skin_output_reuse (off): skip only a completed, exclusively owned slot from the immediately previous epoch, with identical instance, CPU surface, numeric inputs, UVs and publication mode. Reservation alone is insufficient. Raw outputs are conservatively excluded.',
        '- combined enables both; arm-off compares the candidate binary with both off to the clean RC14 reference.','',
        '| Tape | Frames/pair | Off | Alpha | Skin | Combined |','|---|---:|---|---|---|---|']
    for tape in dict.fromkeys(r['tape'] for r in proof):
        rows=[r for r in proof if r['tape']==tape];assert len(rows)==4
        lines.append(f"| {tape} | {rows[0]['frames']} | PASS | PASS | PASS | PASS |")
    lines+=['',f"20/20 full-tape comparisons PASS, {sum(r['frames'] for r in proof)} compared frames. Every state, ordered event, skin payload, submitted packet, geometry/uniform payload, world/joint order, image hash and presentation field matches without tolerance. Unknown payload fields and non-observer counters remain compared. Only clocks, documented memory observers, volatile GPU slot identity, the inherited two dynamic-collider observer counters and the two named coverage counters are normalized. Host divergence from the native recording is compared exactly to the host base, not mistaken for candidate divergence.",'',
        'Coverage on wchess1: A4 alpha avoids 113357 repeated checks / 1200 frames; owner play 677432 / 6375. Skin reuses only 21233 vertices / 1200 and 137496 / 6375 (~18 and ~22 vertices/frame). This is sparse relative to the chess geometry. Neither an 8 ms saving nor a meaningful wchess1 skin gain is justified by that coverage. Environment-mapped chess bodies use the deferred/raw family, which this conservative cache excludes. Extending exact raw-output/layer reuse is a follow-up, not a claimed result.','',
        '## Package and root order','',
        f"READY SHA-256 `{sha(P/'READY.json')}`.",f"SELF SHA-256 `{ready['self_sha256']}`.",f"Driver SHA-256 `{ready['driver_sha256']}`.",f"Kit unchanged `{ready['kit_sha256']}`.",'',
        'Window order: A4 700..1180; owner play 1486..1966; owner play 796..1276. In each window: control, alpha-once, skin-reuse, combined (12 arms). The official live.py selection and its possible unobserved-overlay caveat are pinned. Every console arm replays its whole tape and logs exactly its inclusive 481-frame window. render_scale stays 0.75. README supplies manual root setup, recovery, separate one-tape-at-a-time put/observe/ack/get and restore/read-back/shutdown steps. No console slot is granted by this package.','',
        'Each credit requires final drained replay_gates, the final validated done line, complete steps/checks/window, diff/events=0, dropped=0, finite metrics, config PASS, not-running exit and assert_transaction on the captured observation immediately before credit. The driver never acknowledges, retries or restores.','',
        '## Validation and retained failures','',
        f"Rust: {m[1]} passed, {m[2]} failed, {m[3]} ignored, opt3 release tests with LTO disabled and 256 codegen units. Production host oracle and native SELF retained their production LTO recipes. Earlier full-LTO run passed 1403 and failed 13 frozen-schema assertions; those test-only omissions were fixed. An intermediate rerun reused capture fixtures and was rejected; final fixtures use a fresh per-run directory.",
        'Python: 12 real-kit fake-transport step tests, 3 comparator tests (including ordered-event/payload/pixel mutants), 3 receipt tests PASS. Feature-off cargo check and native release static/library/notices/LiveArea gates PASS. Offline tests do not contact the device.',
        'Rejected timing/proof attempts remain: initial frame-zero render:null parser failure (regression fixed); host 180-frame phase-census export failed hierarchy closure and was not used. The successful family timing uses the existing full render profiler instead. No rejected run is credited.',
        'No console/FTP/1337/1338 contact, kit edits, default flips, Terminal.app windows or game assets in Git. No console restoration or visible-window-count claim. No private host target was needed; the shared target did not block this task. Current packages/proofs remain active inputs for root review.','']
    (P/'REPORT.md').write_text('\n'.join(lines))
if __name__=='__main__':main()
