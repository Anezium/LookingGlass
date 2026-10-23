from pathlib import Path
import pilot as p
import json,sys,subprocess,csv,re,time,math
from run_health import health_text, arm_for_label, entry_for_label
ROOT=p.ROOT
PACKAGE=ROOT.parent
PY='/Users/anezium/vita-j0/venv/bin/python'
STAGER='/Users/anezium/vita-j0/bin/vita-stage'
SELF_HASH=None
def stage_manifest(manifest,ledger):
    m=json.loads(Path(manifest).read_text())
    for v in m['files']:
        assert v['remote']==p.A+'eboot.bin' or v['remote'] in [p.R+x for x in ['vita_renderer.json','vita_sim.json','native-timing-config.json','profile_mode']]
        assert v['kind'] in ['small','self']
    Path(str(ledger)+'.manifest.json').write_text(json.dumps(m,indent=2)+'\n')
    with Path(str(ledger)+'.log').open('w') as out:
        subprocess.run([PY,STAGER,'--manifest',str(manifest),'--ledger',str(ledger),'--verify-hash','--chunk-size','1048576','--log',str(ledger)+'.ftp.jsonl'],stdout=out,stderr=subprocess.STDOUT,check=True)
    return m
def baseline():return json.loads((ROOT/'BASELINE.json').read_text())
def saves(tag):
    p.B=ROOT
    orig={v['path']:v for v in baseline()['manifest'] if '/saves/' in v['path'] or '/route-saves/' in v['path']}
    current=p.snapshot(tag+'-before',list(orig))
    with p.Session(timeout=30) as s:
        for r,v in orig.items():
            c=next(x for x in current if x['path']==r)
            if v['present'] and v['sha256']!=c.get('sha256'):s.execute('put',r,v['local'])
        for d,old in baseline()['inventories'].items():
            if '/saves/' not in d and '/route-saves/' not in d:continue
            for n,v in p.listing(s,d).items():
                if n not in old:
                    assert not v['directory'];s.ftp.delete(d+n);s.record('operation',op='delete',remote=d+n,ok=True)
    read=p.snapshot(tag,list(orig))
    for x in read:assert (x['present'],x.get('sha256'))==(orig[x['path']]['present'],orig[x['path']].get('sha256'))
    independent=p.snapshot(tag+'-independent',list(orig))
    assert [(x['path'],x['present'],x.get('sha256')) for x in read]==[(x['path'],x['present'],x.get('sha256')) for x in independent]
    p.rec(tag+'/SAVES.json',dict(status='PASS',files=read,source='fresh session-start real save bytes'))
    return read
def stage(label,arm,mode='full'):
    assert arm in ['A','B','human-restore'] and mode in ['full','fullbench']
    if arm=='human-restore':
        assert label=='human-restore' and mode=='full', 'Restoration is session-wide, never a bench arm'
        # Keep the exhaustive restoration order. No experimental manifest or STAGE pass.
        for helper in ['restore-ledgers.py','verify-self.py','restore-files.py']:
            subprocess.run([PY,str(ROOT/helper)],check=True)
        return
    assert arm_for_label(label,PACKAGE/'CONSOLE-PROTOCOL.json')==arm
    assert mode=='full', 'Use arm-mode-only.py for fullbench arming'
    p.B=ROOT/label;p.B.mkdir(exist_ok=True)
    assert (ROOT/'BASELINE.json').exists()
    entry=entry_for_label(label,PACKAGE/'CONSOLE-PROTOCOL.json')
    if entry['map']=='pandemonium' and mode=='full':
        from map_identity import save_identity,verify_save_receipt
        frozen=json.loads((ROOT/'PANDEMONIUM-ENTRY.json').read_text())
        verify_save_receipt(frozen)
        restored=saves('saves-before-'+label);p.B=ROOT/label
        auto=next(x for x in restored if x['path']==p.R+'saves/auto.json')
        receipt=dict(save_identity(Path(auto['local']).read_bytes()),fresh_session_start_readback=True,
            companions_restored_and_verified=True,independent_readbacks=2,frozen_receipt=str(ROOT/'PANDEMONIUM-ENTRY.json'))
        verify_save_receipt(receipt);p.rec('SAVE-IDENTITY.json',receipt)
    if mode=='full':
        with p.Session(timeout=30) as s:p.rec('prelaunch-profile.json',p.listing(s,p.R+'frame_profile/'))
    manifest=PACKAGE/f"stage-{entry['config_arm']}-{mode}.json"
    ledger=p.B/f'small-{mode}-ledger.json';m=stage_manifest(manifest,ledger)
    read=p.snapshot('config-readback-'+mode,[v['remote'] for v in m['files']])
    cfg=None
    for x,v in zip(read,m['files']):
        source=Path(v['local'])
        if not source.is_absolute():source=manifest.parent/source
        assert x['present'] and Path(x['local']).read_bytes()==source.read_bytes()
        if v['remote']==p.R+'vita_renderer.json':
            cfg=json.loads(source.read_text());p.rec('inputs/vita_renderer.json',cfg)
    if mode=='full':p.rec('STAGE.json',dict(status='PASS',arm=arm,level='off',self_hash=json.loads((PACKAGE/'packages'/entry['package']/'VERIFICATION.json').read_text())['self_sha256'],targets=read,ledger=str(ledger),FTP_closed=True))
    else:p.rec('BENCH-ARMED.json',dict(time=p.now(),host_us=time.monotonic_ns()//1000,mode='fullbench',FTP_closed=True,ledger=str(ledger)))
    print('STAGE PASS',label,arm,mode,flush=True)
def collect(label):
    p.B=ROOT/label;mapname=entry_for_label(label,PACKAGE/'CONSOLE-PROTOCOL.json')['map']
    names=[f'{mapname}-fullbench-light-120.csv',f'{mapname}-fullbench-light-120-doors.csv']
    a=p.snapshot('exports',[p.R+'frame_profile/'+n for n in names]);b=p.snapshot('exports-independent-readback',[p.R+'frame_profile/'+n for n in names])
    assert [(x['present'],x.get('sha256')) for x in a]==[(x['present'],x.get('sha256')) for x in b]
    assert all(x['present'] for x in a)
    rows=list(csv.DictReader(Path(a[0]['local']).open()));doors=list(csv.DictReader(Path(a[1]['local']).open()))
    assert len(rows)==120 and [int(x['frame']) for x in rows]==list(range(1,121))
    assert all(int(x['wall_us'])>0 for x in rows)
    assert sum(int(x['frames']) for x in doors)==120
    nxt=1
    for x in doors:
        assert int(x['frame_first'])==nxt and int(x['frame_last'])-int(x['frame_first'])+1==int(x['frames']);nxt=int(x['frame_last'])+1
    assert nxt==121
    capture_log(label)
    p.rec('COLLECT.json',dict(time=p.now(),files=a,required=names))
    p.rec('VALIDATION.json',dict(status='PASS_EXPORT_INTEGRITY',samples=120,door_samples=120,positive_wall=True,frame_cpu_us_unavailable=True))
    print('COLLECT PASS',label,flush=True)
def capture_log(label):
    p.B=ROOT/label
    data=Path('/Users/anezium/vita-j0/work/vita.log').read_bytes();offset=json.loads((p.B/'LAUNCH.json').read_text())['log_offset']
    (p.B/'runtime-log.txt').write_bytes(data[offset:])
if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description='Library-only session support. Import stage, collect, capture_log or saves; no campaign CLI.')
    parser.parse_args()
    parser.error('Import this module; never execute a session campaign.')
