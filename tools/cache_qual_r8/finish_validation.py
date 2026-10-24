"""Validate final local evidence and exact tested ZIP; no console access."""
from pathlib import Path
import datetime, hashlib, json, os, shutil, subprocess, zipfile
O=Path(__file__).resolve().parents[1]
R=Path('/Users/anezium/vita-work/lg-cache-qual-r8')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text())
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8')
for src,dest in [('/tmp/alice-r8-e2e-main-final.json','SESSION-E2E-MAIN.json'),('/tmp/alice-r8-e2e-zip-final.json','SESSION-E2E-ZIP.json')]:
    v=read(Path(src));assert v['status']=='PASS' and v['final_status']=='PASS_ORIGINAL_STATE_VERIFIED'
    assert v['real_package_session_path_absent'] and not v['device_contact'] and not v['driver_contact']
    assert all(n['status']=='REJECTED_AS_INTENDED' for n in v['negative_fixtures'])
    names={n['fixture'] for n in v['negative_fixtures']}
    assert {'missing native reload','wrong operation order','entry pose >1 unit',
        'non-modal overlay Circle gives empty window','driver lost before restoration',
        'save restoration after stop while running','in-game Quit after restoration starts',
        'post-load zero sampled hits reload-post','post-load zero sampled hits transition-post',
        'failed motion audit preserves per-pulse diagnostics'}<=names
    shutil.copyfile(src,O/'proof'/dest)
if Path('/tmp/alice-r8-e2e-preseal.json').exists():shutil.copyfile('/tmp/alice-r8-e2e-preseal.json',O/'proof/SESSION-E2E-PRESEAL-IDENTITY-FAILURE.json')
assert not os.path.lexists(O/'session-s8-cache-qual')
for path in [O/'target-vita',O/'tmp',R/'target']:assert not path.exists(),path
for root in [O,R]:assert not list(root.rglob('__pycache__'))
identity=read(O/'PACKAGE-IDENTITY.json');verification=read(O/'proof/PACKAGE-VERIFICATION.json')
assert identity['sealed_brief_sha256']==sha(O/'CONSOLE-BRIEF.md')==sha(R/'tools/cache_qual_r8/CONSOLE-BRIEF.md')
assert read(O/'proof/SESSION-E2E-ZIP.json')['tested_zip_sha256']==sha(O/'CONSOLE-PACKAGE.zip')==verification['zip_sha256']
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip') as z:
    assert z.testzip() is None and set(z.namelist())==set(verification['members'])
    for n,v in verification['members'].items():
        assert z.read(n)==(O/n).read_bytes() and sha(O/n)==v['sha256']
    assert not any(p.startswith('session-') for n in z.namelist() for p in Path(n).parts[:-1])
tests=read(O/'proof/HOST-TESTS.json');assert len(tests)==5 and all(t['exit_code']==0 for t in tests)
for name in ['HOST-EQUALITY.json','NATIVE-STORAGE.json','REMOTE.json','LOG-INTEGRITY-FIXTURES.json','REVIEW-EVIDENCE.json']:
    assert read(O/'proof'/name)['status']=='PASS'
assert identity['branch_head']==read(O/'proof/REMOTE.json')['head']
subprocess.run(['git','diff','--exit-code',identity['engine_source_revision'],'--','src','Cargo.lock','Cargo.toml','build.rs','.cargo'],cwd=R,check=True)
review=(O/'REVIEW.md').read_text().split('## Amendments (text for a separate file, e.g. `AMENDMENTS-REVIEW.md`; the sealed brief stays unchanged)\n\n',1)[1].split('\n## Verified versus assumed',1)[0]
assert review in (O/'AMENDMENTS-REVIEW.md').read_text()
assert (O/'AMENDMENTS-REVIEW.md').read_bytes()==(R/'tools/cache_qual_r8/AMENDMENTS-REVIEW.md').read_bytes()
assert sha(O/'packages/baseline/eboot.bin')==sha(O/'packages/cache/eboot.bin')=='892d7680285ac9441f81fcb4321706e943dbe19c1fe4f0c349e8e09c949e8ef2'
clean=read(O/'proof/CLEANUP.json');clean['df_after']=subprocess.check_output(['df','-h',str(O)],text=True);write(O/'proof/CLEANUP.json',clean)
write(O/'proof/FINAL-VALIDATION.json',dict(status='PASS',time=datetime.datetime.now().astimezone().isoformat(),
    console_status='NOT_EXECUTED',device_contact=False,driver_contact=False,
    engine_source_revision=identity['engine_source_revision'],tooling_head=identity['branch_head'],
    exact_tested_zip_sha256=sha(O/'CONSOLE-PACKAGE.zip'),sealed_brief_sha256=sha(O/'CONSOLE-BRIEF.md'),
    amendments={f.name:sha(f) for f in O.glob('*AMENDMENT*.md') if f.name!='AMENDMENTS-FIX.md'},
    adopted_review_text_verbatim=True,review_evidence_sha256=sha(O/'proof/REVIEW-EVIDENCE.json'),engine_and_self_unchanged=True,
    negative_fixtures={name:len(read(O/'proof'/name)['negative_fixtures']) for name in ['SESSION-E2E-MAIN.json','SESSION-E2E-ZIP.json']},
    helper_bytes_match_zip=True,host_equality=True,final_native_storage=True,
    offline_main_path='PASS_ORIGINAL_STATE_VERIFIED',offline_zip_path='PASS_ORIGINAL_STATE_VERIFIED',
    log_integrity_fixtures=len(read(O/'proof/LOG-INTEGRITY-FIXTURES.json')['checks']),
    session_directory_absent=True,per_task_caches_absent=True,cache_default=False))
print('Final exact ZIP, source, host, offline restoration and cleanup PASS')
