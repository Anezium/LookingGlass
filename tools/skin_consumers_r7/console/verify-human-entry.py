"""Read the current human SELF twice before the first device write."""
import argparse

def main():
    argparse.ArgumentParser(description=__doc__).parse_args()
    import pilot as p
    p.owner();p.B=p.ROOT
    expected='387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990'
    receipts=[]
    for index in [1,2]:
        local=p.ROOT/f'human-entry-self-{index}.bin'
        with p.Session(timeout=120) as session:
            assert session.execute('size',p.A+'eboot.bin')==37371674
            session.execute('get',p.A+'eboot.bin',str(local))
        assert local.stat().st_size==37371674 and p.sha(local.read_bytes())==expected
        receipts.append(dict(local=str(local),bytes=37371674,sha256=expected))
    p.rec('HUMAN-ENTRY-SELF.json',dict(status='PASS',independent_reads=receipts,FTP_closed=True))
    print('Current human SELF independently verified twice')

if __name__=='__main__':main()
