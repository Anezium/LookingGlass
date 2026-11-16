"""Offline manifest sealing. Refuses absent/failed host or native evidence."""
import hashlib,json,shutil,subprocess
from pathlib import Path
P=Path(__file__).resolve().parent
KIT=P.parent/'rc1-session/rc1kit.py'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    proofs=json.loads((P/'host-proof.json').read_text())
    assert len(proofs)==20 and all(r['exact'] for r in proofs)
    assert {(r['tape'],r['candidate']) for r in proofs}=={(t,a) for t in ['a2-04.lgtape','a4-wchess1-01.lgtape','rw-20261116-wchess1-01.lgtape','a4-skool1-01.lgtape','a4-potears1-01.lgtape'] for a in ['arm-off','alpha-once','skin-reuse','combined']}
    bindings=json.loads((P/'logs/build-host-arm-binding.json').read_text())
    unit=json.loads((P/'unit-receipt.json').read_text())
    feature=json.loads((P/'feature-off-receipt.json').read_text())
    assert feature['result']==0 and feature['head']==unit['head']
    receipt=json.loads((P/'builds/9da106a/BUILD-RECEIPT.json').read_text())
    assert unit['result']==0
    import runtime_equivalence
    runtime_equivalence.verify(receipt['head'],unit['head'])
    assert all(r['arm_binary_sha256']==bindings['binary_sha256'] for r in proofs)
    if receipt['head']!=bindings['head']:
        equivalent=json.loads((P/'runtime-equivalence.json').read_text())
        assert equivalent['oracle_head']==bindings['head'] and equivalent['native_head']==receipt['head']
        source=Path('/Users/anezium/vita-work/lg-wchess1')
        path='src/render/vita_renderer/world_list.rs'
        changed=subprocess.check_output(['git','diff','--name-only',bindings['head'],receipt['head'],'--','src','Cargo.toml','Cargo.lock','build.rs'],cwd=source,text=True).splitlines()
        assert changed==[path]
        old=subprocess.check_output(['git','show',bindings['head']+':'+path],cwd=source,text=True)
        new=subprocess.check_output(['git','show',receipt['head']+':'+path],cwd=source,text=True)
        before='            list_serial: 0, class: 0 }'
        after='            list_serial: 0, alpha_serial: 0, class: 0 }'
        assert old.count(before)==1 and old.replace(before,after)==new
        assert old.index(before)>old.index('#[cfg(test)]\nmod tests {')
    assert sha(Path(receipt['self_path']))==receipt['self_sha256']
    baseline_path=P.parent/'rc14/qual/configs/rc14-release-a2-04.json'
    baseline=json.loads(baseline_path.read_text())
    arms=json.loads((P/'arm-plan.json').read_text())
    for arm in arms:
        config=json.loads((P/arm['config']).read_text())
        expected=dict(baseline)
        expected.update(replay_play=arm['tape'],replay_record='',replay_start='',spike_log_ms=.01,spike_log_first_frame=arm['window'][0],spike_log_last_frame=arm['window'][1],log_frame_time_every=0)
        for key in arm['levers']:expected[key]=arm['lever']==key or arm['lever']=='combined'
        assert config==expected,arm['name']
        assert config['render_scale']==.75
        assert sha(P/arm['config'])==arm['config_sha256']
        source=P.parent/('release-week/play-record/tapes' if arm['tape'].startswith('rw-') else 'a5-heavy/tapes')/arm['tape']
        assert sha(source)==arm['tape_sha256']
    assert sha(KIT)=='93f434a6286b2180bd66d89ffad1394b7f27b5e385296c67a70b2271256558b3'
    for name in ['offline-step.log','oracle.log','unit.log']:
        assert (P/'logs'/name).is_file(),name
    assert 'OK' in (P/'logs/offline-step.log').read_text()
    assert 'test result: ok.' in (P/'logs/unit.log').read_text()
    files=[f for f in P.iterdir() if f.suffix in ('.py','.md') and f.name!='REPORT.md']
    files+=list((P/'console-configs').glob('*.json'))+[P/'arm-plan.json',P/'host-proof.json']
    files += [P/name for name in ['attribution.json','cpu-family.json','runtime-equivalence.json','unit-receipt.json','feature-off-receipt.json','logs/unit.log','logs/feature-off.log','logs/offline-step.log','logs/oracle.log','logs/receipts.log','logs/build-host-base-binding.json','logs/build-host-arm-binding.json','logs/build-host-attribution-binding.json','builds/9da106a/BUILD-RECEIPT.json']]
    ready=dict(package='wchess1-render',head=receipt['head'],host_oracle_head=bindings['head'],unit_test_head=unit['head'],self_path=receipt['self_path'],self_sha256=receipt['self_sha256'],driver_sha256=sha(P/'wchess1-step.py'),kit_path=str(KIT),kit_sha256=sha(KIT),baseline_path=str(baseline_path),baseline_sha256=sha(baseline_path),window_selection_sha256=sha(P.parent/'release-week/play-record/windows.json'),arms=arms,host_oracle=proofs,files={str(f.relative_to(P)):sha(f) for f in sorted(files)},free_bytes=shutil.disk_usage(P).free,no_console_contact=True,no_terminal_windows_created=True)
    (P/'READY.json').write_text(json.dumps(ready,indent=2,sort_keys=True)+'\n')
    print(json.dumps(dict(ready_sha256=sha(P/'READY.json'),self_sha256=ready['self_sha256'],driver_sha256=sha(P/'wchess1-step.py')),indent=2))
if __name__=='__main__':main()
