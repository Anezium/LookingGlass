import argparse

def main():
    parser=argparse.ArgumentParser(description='Verify the earliest SELF ledger then separately read and verify the human SELF')
    parser.add_argument('arguments',nargs='*')
    parser.parse_args()
    import pilot as p,json
    p.B=p.ROOT;l=json.loads((p.ROOT/'self-stage-ledger.json').read_text());o=l['entries'][0]['original'];dst=p.ROOT/'restored-self-independent.bin'
    assert not l.get('wedged'),'NO NETWORK AFTER WEDGE'
    with p.Session(timeout=20) as s:
     n=s.execute('size',p.A+'eboot.bin');assert n==o['size'],('SELF SIZE',n,o)
     s.execute('get',p.A+'eboot.bin',str(dst))
     assert dst.stat().st_size==o['size'] and p.sha(dst.read_bytes())==o['sha256'],'ORIGINAL SELF HASH MISMATCH'
     rows=p.listing(s,p.A)
     for part in l['parts']:assert part.rsplit('/',1)[1] not in rows,('OWNED PART REMAINS',part)
    p.rec('SELF-INDEPENDENT-VERIFY.json',dict(time=p.now(),status='PASS',bytes=n,sha256=p.sha(dst.read_bytes()),original_eboot_matches_fresh_backup=True,vita_stage_restore_status=l.get('restore_status'),all_owned_parts_absent=True))
    print('ORIGINAL SELF INDEPENDENTLY VERIFIED',flush=True)

    expected_bytes=37371674
    expected_sha256='387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990'
    second=p.ROOT/'restored-human-self-second-independent.bin'
    with p.Session(timeout=20) as s:
     n2=s.execute('size',p.A+'eboot.bin');assert n2==expected_bytes
     s.execute('get',p.A+'eboot.bin',str(second))
     assert second.stat().st_size==expected_bytes and p.sha(second.read_bytes())==expected_sha256, 'HUMAN SELF SECOND READBACK MISMATCH'
    p.rec('HUMAN-SELF-SECOND-INDEPENDENT-VERIFY.json',dict(time=p.now(),status='PASS',bytes=n2,sha256=p.sha(second.read_bytes()),local=str(second)))

if __name__=='__main__':
    main()
