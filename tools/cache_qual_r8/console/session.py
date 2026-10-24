"""Import-only session operations, with preservation before every destructive restore."""
from pathlib import Path
import json, subprocess, sys
import pilot as p
from run_health import entry_for_label
ROOT=p.ROOT;PACKAGE=ROOT.parent
PY='/Users/anezium/vita-j0/venv/bin/python';STAGER='/Users/anezium/vita-j0/bin/vita-stage'
def stage_manifest(manifest,ledger):
    p.owner()
    m=json.loads(Path(manifest).read_text())
    allowed={p.A+'eboot.bin'}|{p.R+n for n in ['vita_renderer.json','vita_sim.json','native-timing-config.json','profile_mode','cache_qual_command.json']}
    assert all(v['remote'] in allowed and v['kind'] in ['small','self'] for v in m['files'])
    Path(str(ledger)+'.manifest.json').write_text(json.dumps(m,indent=2)+'\n')
    with Path(str(ledger)+'.log').open('w') as out:
        subprocess.run([PY,STAGER,'--manifest',str(manifest),'--ledger',str(ledger),'--verify-hash','--chunk-size','1048576','--log',str(ledger)+'.ftp.jsonl'],stdout=out,stderr=subprocess.STDOUT,check=True)
    return m
def baseline():return json.loads((ROOT/'BASELINE.json').read_text())
def saves(tag):
    p.B=ROOT
    base=baseline();orig={v['path']:v for v in base['manifest'] if '/saves/' in v['path'] or '/route-saves/' in v['path']}
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
        for helper in ['restore-ledgers.py','verify-self.py','restore-files.py']:
            subprocess.run([PY,str(ROOT/helper)],check=True)
        return
    e=entry_for_label(label);assert e['arm']==arm and mode=='full'
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
    p.B=ROOT/label
    assert not (p.B/'COLLECT.json').exists()
    paths=[p.R+'frame_profile/cache-qual.json']
    a=p.snapshot('exports',paths);b=p.snapshot('exports-independent',paths)
    assert a[0]['present'] and [(v['present'],v.get('sha256')) for v in a]==[(v['present'],v.get('sha256')) for v in b]
    initial=next(v for v in baseline()['manifest'] if v['path']==paths[0])
    assert not initial['present'] or initial['sha256']!=a[0]['sha256'],'stale initial export'
    for earlier in ROOT.glob('*/COLLECT.json'):
        assert json.loads(earlier.read_text())['files'][0]['sha256']!=a[0]['sha256'],'stale arm export'
    capture_log(label);p.rec('COLLECT.json',dict(files=a,status='PASS'))
if __name__=='__main__':
    import argparse
    a=argparse.ArgumentParser(description=__doc__);a.add_argument('action',choices=['stage','collect','restore','saves']);a.add_argument('label',nargs='?');args=a.parse_args()
    if args.action=='stage':stage(args.label,entry_for_label(args.label)['arm'])
    elif args.action=='collect':collect(args.label)
    elif args.action=='saves':saves(args.label)
    else:stage('human-restore','human-restore')
