#!/usr/bin/env python3
"""Root-only recording steps. No kit source changes and no automatic observation."""
import argparse, hashlib, json, re, subprocess
from pathlib import Path
PIN = '93f434a6286b2180bd66d89ffad1394b7f27b5e385296c67a70b2271256558b3'
DEFAULT_KIT = '/Users/anezium/vita-j0/plan-2026-11-12/rc1-session/rc1kit.py'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser()
    p.add_argument('action',choices=['start','recover','get','restore'])
    p.add_argument('--session',type=Path,required=True)
    p.add_argument('--kit',type=Path,default=Path(DEFAULT_KIT))
    p.add_argument('--kit-sha256',default=PIN,help='Only override for the hermetic fake-kit test')
    p.add_argument('--self',dest='binary',type=Path)
    p.add_argument('--config',type=Path)
    a=p.parse_args(); root=a.session.resolve(); marker=root/'fight-record-active.json'
    assert sha(a.kit)==a.kit_sha256, 'kit hash mismatch'
    def run(*args):
        r=subprocess.run(['python3',str(a.kit),'--session',str(root),*map(str,args)],capture_output=True,text=True,timeout=240)
        print(r.stdout,end='');print(r.stderr,end='')
        assert r.returncode==0 and 'INCONCLUSIVE' not in r.stdout+r.stderr, 'kit failed; recover through root protocol'
        return r.stdout
    def clean_record():
        active=json.loads(marker.read_text()); log=(root/'logs/arms.log').read_bytes()
        assert len(log)>=active['log_offset'], 'receiver log replaced'
        text=log[active['log_offset']:].decode(errors='replace')
        pattern=r'\[lg/replay\] recorded '+re.escape(active['tape'])+r' steps=([1-9][0-9]*) reason=menu checks=([1-9][0-9]*) '
        assert len(re.findall(pattern,text))==1, 'missing unique clean menu recording; no recovery/get authorized by this step'
        assert not any(s in text.lower() for s in ['recording failed','closing '+active['tape']+' failed','panicked at','invalid config']), 'record failed'
        return active
    if a.action=='start':
        assert not marker.exists(), 'one tape at a time'
        assert a.binary and a.config
        c=json.loads(a.config.read_text()); tape=c['replay_record']
        assert c['replay_fight_pilot'] is True and not c['replay_play'] and c['replay_start'] in tuple(f'chapter:{m}@normal' for m in ('hatter2','centipede1','skool1','grounds1','grounds2','jlair2','potears3','qlair','hatter1','fortress1','utemple','wchess2'))
        assert re.fullmatch(r'[a-zA-Z0-9_.-]+\.lgtape',tape)
        state=json.loads((root/'state.json').read_text())
        assert state['game']=='closed' and not state.get('pending'), 'root must observe/acknowledge closed first'
        log=root/'logs/arms.log'; offset=log.stat().st_size
        marker.write_text(json.dumps(dict(tape=tape,log_offset=offset,self_sha256=sha(a.binary),config_sha256=sha(a.config)),indent=2)+'\n')
        run('install',a.binary.resolve(),a.config.resolve())
        run('launch')
        output=run('log','check-config')
        assert re.search(r'^RESULT:\s*PASS(?:[:\s]|$)',output,re.M), 'config was not verified'
    elif a.action=='recover':
        clean_record();run('recover')
        print('Root must inspect the returned PNG, then use kit closed --observed before get.')
    elif a.action=='get':
        active=clean_record();state=json.loads((root/'state.json').read_text())
        assert state['game']=='closed' and not state.get('pending'), 'root observation is required'
        run('tape','get',active['tape'])
        # Keep the active receipt; never silently reuse a take name.
        marker.rename(root/(active['tape']+'.receipt.json'))
    else:
        assert not marker.exists(), 'finish or explicitly resolve the active take first'
        run('restore','--keep-saves')
if __name__=='__main__': main()
