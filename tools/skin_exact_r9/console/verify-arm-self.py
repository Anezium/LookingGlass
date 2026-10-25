"""LABEL; independently read the fixed SELF before each fresh launch."""
import argparse

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('label')
    args=parser.parse_args()
    import pilot as p,json
    from run_health import entry_for_label
    entry=entry_for_label(args.label)
    p.owner();p.B=p.ROOT/args.label;p.B.mkdir(exist_ok=True)
    expected=json.loads((p.ROOT.parent/'packages'/entry['package']/'VERIFICATION.json').read_text())
    remote=p.A+'eboot.bin';local=p.B/'self-independent-readback.bin'
    with p.Session(timeout=120) as session:
        assert session.execute('size',remote)==expected['self_bytes']
        session.execute('get',remote,str(local))
    assert local.stat().st_size==expected['self_bytes']
    assert p.sha(local.read_bytes())==expected['self_sha256']
    p.rec('SELF-IDENTITY.json',dict(status='PASS',local_readback_path=str(local.resolve()),
        remote=remote,bytes=local.stat().st_size,sha256=expected['self_sha256'],package=entry['package'],FTP_closed=True))
    print('SELF identity PASS',args.label)

if __name__=='__main__':main()
