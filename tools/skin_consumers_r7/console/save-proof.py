import argparse

def main():
    parser=argparse.ArgumentParser(description='Record the freshly preserved Pandemonium save identity')
    parser.add_argument('arguments',nargs='*')
    parser.parse_args()
    import pilot as p,json
    f=p.ROOT/'originals/data/alice/private/saves/auto.json'
    from map_identity import save_identity,verify_save_receipt
    independent=p.ROOT/'originals-independent-readbacks/data/alice/private/saves/auto.json'
    assert f.read_bytes()==independent.read_bytes()
    identity=save_identity(f.read_bytes());current=identity['visit_key']
    base=json.loads((p.ROOT/'BASELINE.json').read_text())
    for row in base['manifest']:
        if '/saves/' in row['path'] or '/route-saves/' in row['path']:
            if row['present']:
                a=p.Path(row['local'])
                b=p.ROOT/'originals-independent-readbacks'/row['path'].removeprefix('ux0:/')
                assert p.sha(a.read_bytes())==row['sha256']==p.sha(b.read_bytes())
    receipt=dict(identity,time=p.now(),path=p.R+'saves/auto.json',current=current,
        fresh_session_start_readback=True,companions_restored_and_verified=True,independent_readbacks=2,
        entry='Load Save current real autosave',source='two fresh session-start device reads; no historical substitution')
    verify_save_receipt(receipt);p.rec('PANDEMONIUM-ENTRY.json',receipt)
    session=json.loads((p.ROOT/'SESSION.json').read_text());session.pop('initial_quit',None);session['initial_state']='Fresh HOME inspected, no game visible; no quit input needed.';p.rec('SESSION.json',session)
    print(current,p.sha(f.read_bytes()))

if __name__=='__main__':
    main()
