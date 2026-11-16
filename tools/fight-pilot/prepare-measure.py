#!/usr/bin/env python3
"""Prepare fight-tape RC14 windows offline with native tape-get provenance."""
import argparse,hashlib,json,re
from pathlib import Path
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def index(path):
    data=path.read_bytes()
    assert len(data)<=32*1024*1024 and data.endswith(b'\n'), 'oversize/truncated tape'
    lines=data.decode().splitlines(); assert lines.pop(0)=='LGTAPE 1'
    maps=[]; checks=0; name=None; end=None; header=None
    for line in lines:
        tag, _, rest=line.partition(' ')
        assert end is None or tag=='M', 'data after END'
        if tag=='H':
            assert header is None; header=json.loads(rest)
            assert header['version']==1 and header['platform']=='vita'
        elif tag=='E':
            step,event=rest.split(' ',1); assert int(step)==len(maps)
            if event.startswith('map:'): name=event[4:]
        elif tag=='C':
            assert int(rest.split()[0])==len(maps); checks+=1
        elif tag=='F':
            assert name and re.match(r'^[0-9a-f]{8}( |$)',rest)
            maps.append(name)
        elif tag=='END': end=json.loads(rest)
        elif tag not in ('S','M'): raise ValueError('unknown tape row')
    assert header and end and 0<len(maps)<=432000
    assert end['steps']==len(maps) and end['checks']==checks and checks>0
    assert end['reason'] in ('menu','exit','step-limit'), 'failed recording'
    return dict(steps=len(maps),checks=checks,maps=maps,sha256=sha(path))

def main():
    p=argparse.ArgumentParser();p.add_argument('--session',required=True,type=Path);p.add_argument('--windows',required=True,type=Path);p.add_argument('--rc14-self',required=True,type=Path)
    p.add_argument('--kit',type=Path,default=Path('/Users/anezium/vita-j0/plan-2026-11-12/rc1-session/rc1kit.py'))
    p.add_argument('--kit-sha256',default='93f434a6286b2180bd66d89ffad1394b7f27b5e385296c67a70b2271256558b3')
    p.add_argument('--rc14-sha256',default='966eac903585a56df7bbe5a015a1f9b620d0bd4923ea5be9d1965742b919bca0')
    a=p.parse_args();root=a.session.resolve()
    assert sha(a.kit)==a.kit_sha256,'pinned kit hash mismatch'
    assert sha(a.rc14_self)==a.rc14_sha256,'RC14 SELF hash mismatch'
    assert not (root/'READY.json').exists(), 'existing measurement manifest'
    base=json.loads((Path(__file__).resolve().parents[2]/'configs/PL-rc13-release-play04.json').read_text())
    assert not base.get('replay_fight_pilot',False)
    (root/'baseline.json').write_text(json.dumps(base,indent=2)+'\n')
    selection=json.loads(a.windows.read_text());records=[json.loads(l) for l in (root/'session.jsonl').read_text().splitlines() if l.strip()]
    tapes={};arms=[];occupied={}
    (root/'configs').mkdir(exist_ok=True)
    for row in selection['windows']:
        name=row['tape'];assert re.fullmatch(r'fp-[a-zA-Z0-9_.-]+\.lgtape',name)
        path=root/'tapes'/name;t=index(path);tapes[name]=t
        assert any(r.get('command')=='tape get' and r.get('local')==str(path) and r.get('sha256')==t['sha256'] for r in records), 'missing kit retrieval provenance'
        first,last=row['window'];assert 1<=first<=last<=t['steps'] and last-first<481
        used=occupied.setdefault(name,set());frames=set(range(first,last+1));assert not used&frames,'overlapping windows';used.update(frames)
        label=f'{path.stem}-{first:06}-rc14'
        config=dict(base,replay_record='',replay_play=name,replay_start='',replay_stop_after=0,log_frame_time_every=0,spike_log_ms=.001,spike_log_first_frame=first,spike_log_last_frame=last)
        file=root/'configs'/f'{label}.json';file.write_text(json.dumps(config,indent=2)+'\n')
        arms.append(dict(name=label,variant='rc14',tape=name,tape_sha256=t['sha256'],steps=t['steps'],checks=t['checks'],window=[first,last],config=str(file.relative_to(root)),config_sha256=sha(file)))
    assert arms,'no windows selected'
    (root/'tape-index.json').write_text(json.dumps(tapes,indent=2)+'\n')
    manifest=dict(kit_sha256=sha(a.kit),baseline_sha256=sha(root/'baseline.json'),tape_index_sha256=sha(root/'tape-index.json'),selection=selection,selection_sha256=sha(a.windows),selves=dict(rc14=dict(path=str(a.rc14_self.resolve()),sha256=sha(a.rc14_self))),arms=arms)
    (root/'READY.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(f'{len(tapes)} native tapes; {len(arms)} RC14 windows; offline preparation only')
if __name__=='__main__':main()
