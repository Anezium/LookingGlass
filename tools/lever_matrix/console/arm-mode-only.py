import argparse

def main():
    parser=argparse.ArgumentParser(description='LABEL; arm fullbench using only the mode manifest')
    parser.add_argument('arguments',nargs='*')
    parser.parse_args()
    import pilot as p,session as c,json,time,sys
    label=sys.argv[1];p.B=p.ROOT/label
    from run_health import arm_for_label, entry_for_label
    arm=arm_for_label(label)
    staged=json.loads((p.B/'STAGE.json').read_text())
    assert staged['status']=='PASS' and staged['arm']==arm
    from map_identity import verify_runtime
    from session import capture_log
    capture_log(label)
    verify_runtime((p.B/'runtime-log.txt').read_text(errors='replace'),entry_for_label(label),frozen=False)
    manifest=p.ROOT.parent/'stage-fullbench-mode-only.json'
    ledger=p.B/'small-fullbench-mode-only-ledger.json'
    c.stage_manifest(manifest,ledger)
    entry=entry_for_label(label)
    full=json.loads((p.ROOT.parent/('stage-'+entry['config_arm']+'-full.json')).read_text())
    for x in full['files']:
     if x['remote']==p.R+'profile_mode':x['local']=str(p.ROOT.parent/'configs/profile_mode-fullbench.txt')
    read=p.snapshot('config-readback-fullbench',[x['remote'] for x in full['files']])
    for x,v in zip(read,full['files']):assert p.Path(x['local']).read_bytes()==p.Path(v['local']).read_bytes()
    p.rec('BENCH-ARMED.json',dict(time=p.now(),host_us=time.monotonic_ns()//1000,mode='fullbench',FTP_closed=True,ledger=str(ledger),unchanged_configs='independent fixed-path readback, no redundant replacement'))
    print('MODE-ONLY FULLBENCH / ALL FOUR CONFIG READBACKS PASS',flush=True)

if __name__=='__main__':
    main()
