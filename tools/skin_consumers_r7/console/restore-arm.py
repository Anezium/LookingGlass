import argparse

def main():
    parser=argparse.ArgumentParser(description="LABEL; restore this arm's small ledgers only")
    parser.add_argument('arguments',nargs='*')
    parser.parse_args()
    import sys,json,subprocess
    import pilot as p,session as c
    from run_health import arm_for_label
    arm_for_label(sys.argv[1])
    run=p.ROOT/sys.argv[1]
    for ledger in sorted(run.glob('*ledger.json'), key=lambda x:x.stat().st_mtime, reverse=True):
     d=json.loads(ledger.read_text());assert not d.get('wedged')
     assert all(entry.get('remote',entry.get('path'))!=p.A+'eboot.bin' for entry in d.get('entries',[])), 'SELF ledgers belong at session ROOT'
     if d.get('restore_status')=='restored':continue
     with ledger.with_suffix('.restore.log').open('w') as out:
      subprocess.run([c.PY,c.STAGER,'--restore',str(ledger),'--verify-hash','--chunk-size','1048576','--log',str(ledger)+'.restore.ftp.jsonl'],stdout=out,stderr=subprocess.STDOUT,check=True)
    p.B=run
    full=json.loads((p.ROOT.parent/'stage-baseline-full.json').read_text())
    p.rec('POST-ARM-CONFIG-READBACK.json',p.snapshot('post-arm-configs',[v['remote'] for v in full['files']]))
    print('Arm small ledgers restored through stager; owned recovery partials cleaned')

if __name__=='__main__':
    main()
