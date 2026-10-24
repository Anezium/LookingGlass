"""Final local provenance, cache cleanup and artifact seal. No device transport."""
from pathlib import Path
import ast,hashlib,json,shutil,subprocess
O=Path(__file__).resolve().parents[1]
R=Path('/Users/anezium/vita-work/lg-lever-matrix-r3')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def git(*a):return subprocess.check_output(['git','-C',str(R),*a],text=True).strip()
def write(p,data):p.write_text(json.dumps(data,indent=2)+'\n')
assert git('status','--porcelain')==''
head=git('rev-parse','HEAD');branch=git('branch','--show-current')
remote=git('ls-remote','origin','refs/heads/'+branch).split()[0];assert remote==head
for p in [*O.joinpath('console').glob('*.py'),*O.joinpath('scripts').glob('*.py')]:ast.parse(p.read_text(),filename=str(p))
for proof in ['ROUND3-DRY-RUN.json','COPIED-HELPER-BINDING.json','CONSOLE-TOOLS-TESTS.json','MATRIX-CONTRACT-TESTS.json','ZERO-GL-HOOKS.json','LOGGING-PROOF.json']:
    assert json.loads((O/'proof'/proof).read_text())['status']=='PASS'
for directory in ['console','configs','scripts']:
    for p in (R/'tools/lever_matrix_r3'/directory).glob('*'):
        if p.is_file():assert p.read_bytes()==(O/directory/p.name).read_bytes(),p
for p in [O/'CONSOLE-BRIEF.md',O/'DESIGN.md',O/'CONSOLE-PROTOCOL.json',*O.glob('stage-*.json')]:
    assert p.read_bytes()==(R/'tools/lever_matrix_r3'/p.name).read_bytes(),p
vpath=O/'packages/baseline/VERIFICATION.json';v=json.loads(vpath.read_text())
for package in (O/'packages').iterdir():
    proof=package/'VERIFICATION.json';data=json.loads(proof.read_text())
    data['tooling_revision']=head
    write(proof,data)
matrix=json.loads((O/'PACKAGE-MATRIX.json').read_text())
matrix['packages']=[json.loads((O/'packages'/p['arm']/'VERIFICATION.json').read_text()) for p in matrix['packages']]
write(O/'PACKAGE-MATRIX.json',matrix)
write(O/'proof/TOOLING-SOURCE.json',dict(branch=branch,revision=head,remote_revision=remote,build_revision=v['revision'],plain_push=True,
    repository='https://github.com/Anezium/LookingGlass',engine_base='a859ceea2e35fdffec97872bc9cea46956ccbbd2',
    protected_document_changed=bool(git('diff','a859cee','HEAD','--','docs/RCHESS1.md')),
    engine_changed_files=git('diff','--name-only','a859cee','HEAD','--','src').splitlines(),
    new_repository_assets=[],console_contact=False,driver_contact=False))

targets=[O/'target-vita',O/'tmp',O/'offline-helper-binding',*O.rglob('__pycache__')]
removed=[]
for path in targets:
    if not path.exists():continue
    assert O in path.parents and path.name in ('target-vita','tmp','offline-helper-binding','__pycache__')
    size=sum(p.stat().st_size for p in path.rglob('*') if p.is_file())
    shutil.rmtree(path);removed.append(dict(path=str(path),bytes=size))
df=subprocess.check_output(['/bin/df','-h','/Users/anezium'],text=True)
write(O/'proof/CLEANUP.json',dict(status='PASS',removed=removed,removed_bytes=sum(x['bytes'] for x in removed),
    shared_host_target='/Users/anezium/vita-j0/cargo-target-host',shared_host_target_retained=Path('/Users/anezium/vita-j0/cargo-target-host').is_dir(),df=df))

report=f'''Completed the Mac-only r3 implementation in [alice-lever-matrix-r3]({O}). No contact with 192.168.1.23 or the driver; P.T. ownership was respected.

Fixed logging in [common engine code]({R}/src/logging.rs). Audio used stderr while benchmark completion used stdout; their separate locks allowed formatting fragments to interleave downstream. Records now include their newline before shared serialization, with both stream locks held through the write. Existing debug transport/file destinations are preserved. Two logger unit tests passed, including 4,000 concurrent records through fragmented dual sinks; the production logger also passed 4,000 records through a pipe and 4,000 through a file. The strict audit is unchanged and still rejects the original S3 B3 evidence.

The world regression is confirmed, but its individual cause remains unresolved. Static merging reduces geometry buffer pairs from 1,513 to 231 while preserving payload, zero per-frame geometry uploads and all four entry snapshots' 304 draws. Historical host single-switch counters show binding savings without draw reduction. Material sorting adds schedule validation/copying and possible rebuild/projection work; merging adds eligibility/coverage scans and changes buffer locality; persistence adds field comparisons while saving uploads. Existing evidence cannot assign the +2.371/+1.631 ms deltas to one switch. No clear renderer defect was demonstrated, so renderer behavior is unchanged. [Diagnosis]({O}/proof/WORLD-DIAGNOSIS.json).

Built six [packages]({O}/packages), each with VERIFICATION.json and the same **37,867,758-byte SELF**, SHA-256:

`2fa379ab42ec144a943a78db22669b00d18a742b39154ba15be315cf18f0a7ad`

Zero-hook resolver/symbol checks passed. Timer source and native instructions/operands match the old build except verified symbol, branch and constant relocations. Baseline config and all non-logging engine sources/dependency pins match qualified B. New-SELF runtime parity is unmeasured. [Source diff]({O}/proof/SOURCE-DIFF.patch) and [ELF/SELF comparison]({O}/proof/SELF-COMPARISON.json) document the change; individual changed SELF byte regions cannot all be causally attributed after relinking.

[CONSOLE-BRIEF.md]({O}/CONSOLE-BRIEF.md) and [DESIGN.md]({O}/DESIGN.md) schedule S3 FX rerun, S4 audio and three independent Pandemonium single-world-switch ABBA sessions. All 28 launches remain PENDING. FX/audio budgets are 127.5 minutes each; each world session is 91.5 minutes, all including 30-minute exhaustive restoration. Total: 529.5 minutes across five independently admitted cycles. **Every block must establish fresh A baselines on the new SELF; old A runs cannot carry forward.** World Hedge3 and interactions remain unmeasured; no promotion is made.

The brief retains every Round 2 amendment, dynamic DRIVER-READY admission, en0 preflight, serial locked FTP, eboot-only app staging, exact human SELF `387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990`, sole real autosave `ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9`, companions, inventories and exhaustive restoration. All 24 console helpers passed offline import/help and behavioral checks in both template and copied layouts: **290 checks per layout, 126 regression fixtures and 157 contracts**. Host timer/resolver, submission, FX and audio selections also passed. No console qualification or audible continuity is claimed.

Plain-pushed [vita-lever-matrix-r3-20261024](https://github.com/Anezium/LookingGlass/tree/{branch}) at `{head}`; build/logging revision `{v['revision']}`, based on `a859cee`. No cargo update, game assets in repositories, protected-document edit or default promotion.

Removed **{sum(x['bytes'] for x in removed):,} bytes** of task caches/test-layout files; retained `~/vita-j0/cargo-target-host`. Final df:

```text
{df.strip()}
```
'''
(O/'SOL-REPORT.md').write_text(report)
seal={str(p.relative_to(O)):sha(p) for p in sorted(O.rglob('*')) if p.is_file() and p.name not in ('ARTIFACT-SHA256.json','FINAL-AUDIT.json','SOL-REPORT.md')}
write(O/'ARTIFACT-SHA256.json',seal)
assert all(sha(O/name)==digest for name,digest in seal.items())
write(O/'FINAL-AUDIT.json',dict(status='PASS',sealed_files=len(seal),seal_sha256=sha(O/'ARTIFACT-SHA256.json'),
    report_sha256=sha(O/'SOL-REPORT.md'),head=head,remote_head=remote,source_clean=True,
    common_self_sha256=v['self_sha256'],console_contact=False,driver_contact=False,all_runs_pending=True,
    scope='Host implementation/preparation complete; individual world cost and all new native runtime results unmeasured'))
print(json.dumps(dict(status='PASS',sealed_files=len(seal),removed_bytes=sum(x['bytes'] for x in removed),report=str(O/'SOL-REPORT.md'))))
