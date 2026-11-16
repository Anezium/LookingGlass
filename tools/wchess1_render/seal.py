"""Offline manifest sealing. Refuses absent/failed host or native evidence."""
import hashlib,json,shutil
from pathlib import Path
P=Path(__file__).resolve().parent
KIT=P.parent/'rc1-session/rc1kit.py'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    proofs=json.loads((P/'host-proof.json').read_text())
    assert len(proofs)==20 and all(r['exact'] for r in proofs)
    assert {(r['tape'],r['candidate']) for r in proofs}=={(t,a) for t in ['a2-04.lgtape','a4-wchess1-01.lgtape','rw-20261116-wchess1-01.lgtape','a4-skool1-01.lgtape','a4-potears1-01.lgtape'] for a in ['arm-off','alpha-once','skin-reuse','combined']}
    bindings=json.loads((P/'logs/build-host-arm-binding.json').read_text())
    receipt=json.loads((P/'builds'/bindings['head'][:7]/'BUILD-RECEIPT.json').read_text())
    assert receipt['head']==bindings['head']
    assert sha(Path(receipt['self_path']))==receipt['self_sha256']
    assert sha(KIT)=='93f434a6286b2180bd66d89ffad1394b7f27b5e385296c67a70b2271256558b3'
    for name in ['offline-step.log','oracle.log','unit.log']:
        assert (P/'logs'/name).is_file(),name
    assert 'OK' in (P/'logs/offline-step.log').read_text()
    assert 'test result: ok.' in (P/'logs/unit.log').read_text()
    files=[f for f in P.iterdir() if f.suffix in ('.py','.md') and f.name!='REPORT.md']
    files+=list((P/'console-configs').glob('*.json'))+[P/'arm-plan.json',P/'host-proof.json']
    ready=dict(package='wchess1-render',head=receipt['head'],self_path=receipt['self_path'],self_sha256=receipt['self_sha256'],kit_path=str(KIT),kit_sha256=sha(KIT),arms=json.loads((P/'arm-plan.json').read_text()),host_oracle=proofs,files={str(f.relative_to(P)):sha(f) for f in sorted(files)},free_bytes=shutil.disk_usage(P).free,no_console_contact=True,no_terminal_windows_created=True)
    (P/'READY.json').write_text(json.dumps(ready,indent=2,sort_keys=True)+'\n')
    print(json.dumps(dict(ready_sha256=sha(P/'READY.json'),self_sha256=ready['self_sha256'],driver_sha256=sha(P/'wchess1-step.py')),indent=2))
if __name__=='__main__':main()
