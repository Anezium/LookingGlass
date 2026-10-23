from pathlib import Path
import pilot as p
from light_csv import read as read_light, summarize as summarize_light
import json,sys,subprocess,csv,re,time,math
ROOT=p.ROOT
PACKAGE=ROOT.parent
PY='/Users/anezium/vita-j0/venv/bin/python'
STAGER='/Users/anezium/vita-j0/bin/vita-stage'
SELF_HASH=json.loads((PACKAGE/'NATIVE-PACKAGE-SHA256.json').read_text())['native-package/looking-glass.self']['sha256']
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
    p.rec(tag+'/SAVES.json',dict(status='PASS',files=read,source='fresh session-start real save bytes'))
def stage(label,arm,mode='full'):
    assert arm in ['baseline','world','fx','audio','reference'] and mode in ['full','fullbench']
    p.B=ROOT/label;p.B.mkdir(exist_ok=True)
    assert (ROOT/'BASELINE.json').exists()
    if 'pandemonium' in label and mode=='full':saves('saves-before-'+label);p.B=ROOT/label
    if mode=='full':
        with p.Session(timeout=30) as s:p.rec('prelaunch-profile.json',p.listing(s,p.R+'frame_profile/'))
    manifest=PACKAGE/f'stage-{arm}-{mode}.json'
    ledger=p.B/f'small-{mode}-ledger.json';m=stage_manifest(manifest,ledger)
    read=p.snapshot('config-readback-'+mode,[v['remote'] for v in m['files']])
    cfg=None
    for x,v in zip(read,m['files']):
        source=Path(v['local'])
        if not source.is_absolute():source=manifest.parent/source
        assert x['present'] and Path(x['local']).read_bytes()==source.read_bytes()
        if v['remote']==p.R+'vita_renderer.json':
            cfg=json.loads(source.read_text());p.rec('inputs/vita_renderer.json',cfg)
    if mode=='full':p.rec('STAGE.json',dict(status='PASS',arm=arm,level='light',self_hash=SELF_HASH if arm!='reference' else '540e73c3a5560f34c14d7d72c2206aa87c853464cb5ecb506b5d6fb4571a3633',targets=read,ledger=str(ledger),FTP_closed=True))
    else:p.rec('BENCH-ARMED.json',dict(time=p.now(),host_us=time.monotonic_ns()//1000,mode='fullbench',FTP_closed=True,ledger=str(ledger)))
    print('STAGE PASS',label,arm,mode,flush=True)
def collect(label):
    p.B=ROOT/label;mapname=label.split('-')[1]
    names=[f'{mapname}-fullbench-light-120.csv',f'{mapname}-fullbench-light-120-doors.csv']
    a=p.snapshot('exports',[p.R+'frame_profile/'+n for n in names]);b=p.snapshot('exports-independent-readback',[p.R+'frame_profile/'+n for n in names])
    assert [(x['present'],x.get('sha256')) for x in a]==[(x['present'],x.get('sha256')) for x in b]
    assert all(x['present'] for x in a)
    rows=list(csv.DictReader(Path(a[0]['local']).open()));doors=list(csv.DictReader(Path(a[1]['local']).open()))
    assert len(rows)==120 and [int(x['frame']) for x in rows]==list(range(1,121))
    observations=read_light(a[0]['local'], 'observed' if 'reference' in label else None)
    availability=summarize_light(observations)
    assert sum(int(x['frames']) for x in doors)==120
    nxt=1
    for x in doors:
        assert int(x['frame_first'])==nxt and int(x['frame_last'])-int(x['frame_first'])+1==int(x['frames']);nxt=int(x['frame_last'])+1
    assert nxt==121
    capture_log(label)
    p.rec('COLLECT.json',dict(time=p.now(),files=a,required=names))
    p.rec('VALIDATION.json',dict(status='PASS_EXPORT_INTEGRITY',samples=120,door_samples=120,positive_wall=True,gl_observers=availability['gl_observers'],unavailable_fields=availability['unavailable_fields'],timing_gate_field='wall_us'))
    print('COLLECT PASS',label,flush=True)
def capture_log(label):
    p.B=ROOT/label
    data=Path('/Users/anezium/vita-j0/work/vita.log').read_bytes();offset=json.loads((p.B/'LAUNCH.json').read_text())['log_offset']
    (p.B/'runtime-log.txt').write_bytes(data[offset:])
def health_text(text,reference=False):
    reasons=[]
    # Audio gates use exactly the r4 camera-freeze-to-done interval.
    bench=text.split('bench: camera frozen map=',1)[1].split('bench: done 120 frames mode=fullbench',1)[0] if 'bench: camera frozen map=' in text else ''
    heap_text=text
    text=bench
    for line in text.splitlines():
        if '[lg/audio-realtime]' in line:
            vals=dict(re.findall(r'(\w+)=([^ ]+)',line))
            if not reference and float(vals.get('producer_mix_p95_ms',0))>=8:reasons.append(dict(kind='producer_mix_p95_ms >=8',line=line))
            for k in ['pcm_starved_blocks','rejected_sinks','dropped_reports']:
                if int(vals.get(k,0))>0:reasons.append(dict(kind=k,line=line))
        if '[lg/audio-music]' in line and re.search(r'underrun_frames=[1-9]',line):reasons.append(dict(kind='music underrun',line=line))
        if '[cpal/vita]' in line and re.search(r'underrun_estimates=[1-9]',line):reasons.append(dict(kind='CPAL underrun',line=line))
    for line in heap_text.splitlines():
        if any(v in line.lower() for v in ["thread 'main' panicked",'memory allocation of','out of memory','fatal error','segmentation fault']):reasons.append(dict(kind='global crash/OOM',line=line))
        for k in ['newlib_uordblks','heap_uordblks_bytes']:
            for v in re.findall(k+r'["\s:=]+(\d+)',line):
                if int(v)>180*1048576:reasons.append(dict(kind='heap >180MiB',line=line))
    return reasons
if __name__=='__main__':
    action=sys.argv[1]
    if action=='stage':stage(*sys.argv[2:])
    elif action=='collect':collect(sys.argv[2])
    elif action=='log':capture_log(sys.argv[2])
    elif action=='saves':saves(sys.argv[2])
    elif action=='health':
        label=sys.argv[2];capture_log(label);p.B=ROOT/label
        reasons=health_text((p.B/'runtime-log.txt').read_text(errors='replace'),reference='reference' in label);p.rec('HEALTH.json',dict(time=p.now(),stop=bool(reasons),reasons=reasons));print(json.dumps(reasons,indent=2));sys.exit(2 if reasons else 0)
