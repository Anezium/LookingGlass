"""Import-only session operations, with preservation before every destructive restore."""
from pathlib import Path
import json, subprocess, sys
import pilot as p
from run_health import entry_for_label
from title_safety import require_closed, failure_restore
ROOT=p.ROOT;PACKAGE=ROOT.parent
PY='/Users/anezium/vita-j0/venv/bin/python';STAGER='/Users/anezium/vita-j0/bin/vita-stage'
def stage_manifest(manifest,ledger):
    p.owner()
    m=json.loads(Path(manifest).read_text())
    if any(v['kind']=='self' for v in m['files']):require_closed('stage SELF')
    allowed={p.A+'eboot.bin'}|{p.R+n for n in ['vita_renderer.json','vita_sim.json','native-timing-config.json','profile_mode','cache_qual_command.json']}
    assert all(v['remote'] in allowed and v['kind'] in ['small','self'] for v in m['files'])
    Path(str(ledger)+'.manifest.json').write_text(json.dumps(m,indent=2)+'\n')
    with Path(str(ledger)+'.log').open('w') as out:
        subprocess.run([PY,STAGER,'--manifest',str(manifest),'--ledger',str(ledger),'--verify-hash','--chunk-size','1048576','--log',str(ledger)+'.ftp.jsonl'],stdout=out,stderr=subprocess.STDOUT,check=True)
    return m
def baseline():return json.loads((ROOT/'BASELINE.json').read_text())
def saves(tag):
    p.B=ROOT
    if failure_restore():require_closed('save restoration after stop/abort',final=True)
    base=baseline();orig={v['path']:v for v in base['manifest'] if '/saves/' in v['path'] or '/route-saves/' in v['path']}
    settings=[p.R+'preferences.json',p.R+'audio-settings.txt']
    expected={v['path']:v for v in base['manifest'] if v['path'] in settings}
    assert set(expected)==set(settings),'settings baseline incomplete'
    settings_a=p.snapshot(tag+'-settings',settings);settings_b=p.snapshot(tag+'-settings-independent',settings)
    for rows in [settings_a,settings_b]:
        assert all((v['present'],v.get('sha256'))==(expected[v['path']]['present'],expected[v['path']].get('sha256')) for v in rows),'Settings drift: preferences.json or audio-settings.txt changed; stop'
    paths=set(orig)
    with p.Session(timeout=30) as s:
        for d in base['inventories']:
            if '/saves/' not in d and '/route-saves/' not in d:continue
            rows=p.listing(s,d)
            assert all(not v['directory'] or n in base['inventories'][d] for n,v in rows.items()),'Unexpected new save directory; preserve and stop'
            paths.update(d+n for n,v in rows.items() if not v['directory'])
    a=p.snapshot(tag+'-created-and-changed',sorted(paths));b=p.snapshot(tag+'-independent',sorted(paths))
    assert [(v['path'],v['present'],v.get('sha256')) for v in a]==[(v['path'],v['present'],v.get('sha256')) for v in b]
    with p.Session(timeout=30) as s:
        for v in a:
            old=orig.get(v['path'])
            if old and old['present']:
                if old['sha256']!=v.get('sha256'):s.execute('put',v['path'],old['local'])
            elif v['present']:s.ftp.delete(v['path']);s.record('operation',op='delete',remote=v['path'],ok=True)
    read=p.snapshot(tag+'-restored',sorted(orig));ind=p.snapshot(tag+'-restored-independent',sorted(orig))
    assert [(v['path'],v['present'],v.get('sha256')) for v in read]==[(v['path'],v['present'],v.get('sha256')) for v in ind]
    assert all((v['present'],v.get('sha256'))==(orig[v['path']]['present'],orig[v['path']].get('sha256')) for v in read)
    p.rec(tag+'/SAVES.json',dict(status='PASS',preserved=a,files=read,independent_readbacks=2))
    return read
def stage(label,arm,mode='full'):
    p.owner()
    if arm=='human-restore':
        assert label=='human-restore'
        require_closed('session.py restore',final=True)
        for helper in ['restore-ledgers.py','verify-self.py','restore-files.py']:
            subprocess.run([PY,str(ROOT/helper)],check=True)
        return
    e=entry_for_label(label);assert e['arm']==arm and mode=='full'
    require_closed('stage arm '+label)
    saves('saves-before-'+label)
    p.B=ROOT/label;p.B.mkdir(exist_ok=False)
    m=stage_manifest(PACKAGE/f"stage-{e['config_arm']}-full.json",p.B/'small-full-ledger.json')
    read=p.snapshot('config-readback',[v['remote'] for v in m['files']])
    for v,item in zip(read,m['files']):
        assert v['present'] and Path(v['local']).read_bytes()==(PACKAGE/item['local']).read_bytes()
    p.rec('STAGE.json',dict(status='PASS',arm=arm,files=read))
def capture_log(label):
    p.B=ROOT/label
    offset=json.loads((p.B/'LAUNCH.json').read_text())['log_offset']
    (p.B/'runtime-log.txt').write_bytes(p.LOG.read_bytes()[offset:])
def collect(label):
    p.owner();p.B=ROOT/label
    assert not (p.B/'COLLECT.json').exists()
    capture_log(label)
    raw=(p.B/'runtime-log.txt').read_bytes()
    p.rec('COLLECT.json',dict(status='PASS',runtime_log_bytes=len(raw),runtime_log_sha256=p.sha(raw)))
if __name__=='__main__':
    import argparse
    a=argparse.ArgumentParser(description=__doc__);a.add_argument('action',choices=['stage','collect','restore','saves']);a.add_argument('label',nargs='?');args=a.parse_args()
    if args.action=='stage':stage(args.label,entry_for_label(args.label)['arm'])
    elif args.action=='collect':collect(args.label)
    elif args.action=='saves':saves(args.label)
    else:stage('human-restore','human-restore')
