import argparse

def main():
    parser=argparse.ArgumentParser(description='Exhaustively verify and restore fresh original files and inventories')
    parser.add_argument('arguments',nargs='*')
    parser.parse_args()
    import pilot as p,json,re
    from title_safety import require_closed
    from map_identity import HUMAN_AUTO
    p.B=p.ROOT
    require_closed('restore-files.py',final=True)
    base=json.loads((p.ROOT/'BASELINE.json').read_text());orig=base['manifest'];inv=base['inventories']
    ledgers=[json.loads(x.read_text()) for x in p.ROOT.rglob('*ledger.json') if '.backups' not in str(x)]
    assert all(x.get('restore_status')=='restored' for x in ledgers),[(x.get('restore_status'),x.get('status')) for x in ledgers]
    caches={r for x in ledgers for r in x.get('cache_created',[])}
    cur=p.snapshot('before-restoration',[v['path'] for v in orig]);by={v['path']:v for v in cur};changes=[]
    changed=[v for v in cur if (v['present'],v.get('sha256'))!=(next(a for a in orig if a['path']==v['path'])['present'],next(a for a in orig if a['path']==v['path']).get('sha256'))]
    assert not any(v['path']==p.A+'sce_sys/param.sfo' for v in changed), 'SFO changed; stop and report without writing or deleting it'
    if changed:
        independent=p.snapshot('before-restoration-independent-readbacks',[v['path'] for v in changed])
        assert [(v['path'],v['present'],v.get('sha256')) for v in changed]==[(v['path'],v['present'],v.get('sha256')) for v in independent]
        p.rec('CHANGED-EVIDENCE-PRESERVED.json',dict(status='PASS',files=changed,independent=independent))
    with p.Session(timeout=120) as s:
        for a in orig:
            c=by[a['path']]
            if a['present'] and a.get('sha256')!=c.get('sha256'):
                assert '/sce_sys/' not in a['path'],'SFO changed; never write'
                assert p.Path(a['local']).stat().st_size<1_000_000,('NOT A SMALL RESTORE',a['path'])
                assert p.sha(p.Path(a['local']).read_bytes())==a['sha256']
                s.execute('put',a['path'],a['local']);changes.append(dict(path=a['path'],op='restore',sha256=a['sha256']))
            elif not a['present'] and c['present']:
                s.ftp.delete(a['path']);s.record('operation',op='delete',remote=a['path'],ok=True);changes.append(dict(path=a['path'],op='restore-absence'))
        current={d:p.listing(s,d) for d in inv}
    p.rec('pre-cleanup-inventories.json',current)
    new=[]
    for d,rows in current.items():
        for n,v in rows.items():
            if n in inv[d] or d+n in caches:continue
            assert not v['directory'],('NEW DIRECTORY',d+n)
            assert d in [p.R+'saves/',p.R+'route-saves/'] or (d==p.R+'frame_profile/' and (n in ['cache-qual.json','cache-qual.tmp'] or re.match(r'(hedge3|hatter1|skool2|pandemonium)-(full|fullbench)-(light-|census-180\.json$)',n))),('UNEXPECTED NEW FILE: do not delete',d+n)
            new.append(d+n)
    if new:
        a=p.snapshot('new-exports-before-cleanup',new);b=p.snapshot('new-exports-independent-readback',new)
        assert [(v['path'],v['sha256']) for v in a]==[(v['path'],v['sha256']) for v in b]
        with p.Session(timeout=120) as s:
            for path in new:s.ftp.delete(path);s.record('operation',op='delete',remote=path,ok=True)
    check=p.snapshot('restoration-readbacks',[v['path'] for v in orig])
    for v in check:
        a=next(a for a in orig if a['path']==v['path']);assert (a['present'],a.get('sha256'))==(v['present'],v.get('sha256')),(a,v)
    with p.Session(timeout=120) as s:final={d:p.listing(s,d) for d in inv}
    for d,rows in final.items():
        actual={k:(v['bytes'],v['directory']) for k,v in rows.items() if d+k not in caches}
        expected={k:(v['bytes'],v['directory']) for k,v in inv[d].items()}
        assert actual==expected,('INVENTORY MISMATCH',d,{k:v for k,v in actual.items() if k not in expected})
    assert json.loads((p.ROOT/'SELF-INDEPENDENT-VERIFY.json').read_text())['status']=='PASS'
    auto=next(v for v in check if v['path']==p.R+'saves/auto.json')
    assert auto['present'] and auto['bytes']==181900 and auto['sha256']==HUMAN_AUTO,'Restored human Auto literal mismatch'
    second=p.snapshot('final-auto-second-independent',[p.R+'saves/auto.json'])[0]
    assert second['present'] and second['bytes']==181900 and second['sha256']==HUMAN_AUTO
    human_self=json.loads((p.ROOT/'HUMAN-SELF-SECOND-INDEPENDENT-VERIFY.json').read_text())
    assert human_self['bytes']==37371674 and human_self['sha256']=='387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990'
    index_before=(p.ROOT/'cache-index-before.json').read_bytes()
    index_current=p.CACHE_INDEX.read_bytes()
    (p.ROOT/'cache-index-before-final-restoration.json').write_bytes(index_current)
    p.CACHE_INDEX.write_bytes(index_before)
    assert p.CACHE_INDEX.read_bytes()==index_before
    p.rec('CACHE-INDEX-RESTORATION.json',dict(status='PASS',before_sha256=p.sha(index_current),restored_sha256=p.sha(index_before)))
    p.rec('RESTORE-FINAL.json',dict(time=p.now(),status='PASS_ORIGINAL_STATE_VERIFIED',human_auto=dict(bytes=181900,sha256=HUMAN_AUTO,independent_readbacks=2),human_self=dict(bytes=human_self['bytes'],sha256=human_self['sha256']),title_closed=True,original_paths_verified=len(check)+1,small_changes=changes,new_exports_collected_removed=new,retained_ledger_owned_caches=sorted(caches),all_initial_absences_verified=True,final_inventories=final))
    print('RESTORE FINAL PASS',len(check)+1,flush=True)

if __name__=='__main__':
    main()
