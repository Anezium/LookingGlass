"""Offline package sealing. Run only once final proofs, images, unit/static gates exist."""
import hashlib,json,shutil,subprocess
from pathlib import Path
P=Path(__file__).resolve().parent
SOURCE=Path('/Users/anezium/vita-work/lg-actor-gpu')
KIT=P.parent/'rc1-session/rc1kit.py'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
def main():
 assert shutil.disk_usage(P).free>41*2**30
 assert sha(KIT)=='93f434a6286b2180bd66d89ffad1394b7f27b5e385296c67a70b2271256558b3'
 proof=json.loads((P/'host-proof.json').read_text());assert len(proof)==14 and all(r['exact'] for r in proof)
 assert len({r['tape'] for r in proof})==7
 pictures=json.loads((P/'picture-proof.json').read_text());assert len(pictures)==4 and all(r['state_events_replay_exact'] for r in pictures)
 tests=json.loads((P/'TEST-RESULTS.json').read_text());assert tests['result']=='OK' and tests['arms_end_to_end']==8
 assert json.loads((P/'unit-receipt.json').read_text())['result']==0
 assert json.loads((P/'feature-off-receipt.json').read_text())['result']==0
 native_path=sorted((P/'builds').glob('*/BUILD-RECEIPT.json'),key=lambda f:f.stat().st_mtime)[-1]
 native=json.loads(native_path.read_text());assert sha(native['self_path'])==native['self_sha256']
 binding=json.loads((P/'logs/build-host-final-binding.json').read_text())
 assert native['head']==binding['head']==json.loads((P/'unit-receipt.json').read_text())['head']
 assert all(r['arm_binary_sha256']==binding['binary_sha256'] for r in proof)
 plan=json.loads((P/'arm-plan.json').read_text());assert len(plan)==8 and plan==json.loads((P/'TEST-MANIFEST.json').read_text())
 base=P/'base/PL-rc15-release-play.json';assert sha(base)=='580f73ba3e3bd166fe2362e2e2a9ab229ad2080d0de8f1036b24d80975585c70'
 for a in plan:
  c=json.loads((P/a['config']).read_text());expected=json.loads(base.read_text());expected.update(a['overrides'])
  assert c==expected and sha(P/a['config'])==a['config_sha256']
  assert c['render_scale']==0.75 and c['gpu_mask']==[] and c['world_gxm']=='live'
  assert not any(v for k,v in c.items() if k.startswith('diag_skip_'))
  assert a['window']==([796,1276] if a['map']=='wchess1' else [376,856])
 scripts=['actor-gpu-step.py','analyze.py','prepare.py','seal.py','verify-session.py','test-step.py','host_oracle.py','test-oracle.py','build_host.py','build_vita.py','run_proof.py','finalize_proof.py','run_picture.py','run_checks.py','capture_counts.py','capture_views.py','attribute.py','verify_census.py','make_screenshots.py','write_report.py','verify_package.py']
 files=[P/name for name in scripts]
 files+=[P/'census-proof.json',P/'picture-interaction.json',P/'census-raster-rejection.json',P/'SCREENSHOTS.md',P/'runtime-equivalence.json',P/'HOST-CLOSURE.json']
 files+=list(native_path.parent.glob('*.txt'))+[native_path.parent/'library-selection.json']
 files+=[P/'census-final'/tape/'BOUNDED-RECEIPT.json' for tape in ['rw-20261116-wchess1-01.lgtape','fp-centipede1-01.lgtape']]

 files+=[P/'README.md',P/'host-proof.json',P/'picture-proof.json',P/'attribution.json',P/'screenshots.json',P/'unit-receipt.json',P/'feature-off-receipt.json',P/'TEST-RESULTS.json',P/'TEST-MANIFEST.json',P/'arm-plan.json',P/'tests/fake-kit.py',native_path,P/'logs/build-host-final-binding.json']
 files+=list((P/'base').glob('*'))+list((P/'console-configs').glob('*.json'))
 tapes={a['tape']:dict(name=a['tape'],sha256=a['tape_sha256'],steps=a['steps'],checks=a['checks'],required=True,source=str(P.parent/('release-week/play-record/tapes' if a['tape'].startswith('rw-') else 'release-week/fight-measure-01/tapes')/a['tape'])) for a in plan}
 r=dict(package='actor-gpu',schema='actor-gpu-1',head=native['head'],branch='vita-actor-gpu-20261117',source_root=str(SOURCE),self_path=native['self_path'],self_sha256=native['self_sha256'],self_bytes=native['self_bytes'],kit_path=str(KIT),kit_sha256=sha(KIT),base_path=str(base),base_sha256=sha(base),driver='actor-gpu-step.py',driver_sha256=sha(P/'actor-gpu-step.py'),arms=plan,tapes=list(tapes.values()),offline=tests,host_binary=binding,pass_rule='Full native replay PASS, diff/events=0, exactly 481 finite ordered spike rows including gxm_end/gxm_end_in, zero dropped logs, config PASS, verified not-running exit, final drained done/gates, assert_transaction under the captured root observation. No automatic acknowledge/retry/restore.',limits=['Native timing/picture unmeasured. No release default promotion.','Host equivalence reproduces existing base divergence from native tapes; host images do not establish Vita picture.','Picture-changing culling requires owner approval; combined inherits that requirement.','No console, FTP, prohibited ports or Terminal.app contact by implementer. Kit never edited.','REPORT.md and FINAL-VERIFICATION.json are post-seal receipts, intentionally outside this non-circular file map.'],files={str(f.relative_to(P)):sha(f) for f in sorted(set(files))},free_bytes=shutil.disk_usage(P).free)
 (P/'READY.json').write_text(json.dumps(r,indent=2,sort_keys=True)+'\n')
 print('READY',sha(P/'READY.json'));print('SELF',r['self_sha256']);print('driver',r['driver_sha256'])
if __name__=='__main__':main()
