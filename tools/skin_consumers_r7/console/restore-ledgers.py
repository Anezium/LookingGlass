import argparse

def main():
    parser=argparse.ArgumentParser(description='Restore all session ledgers in reverse modification-time order')
    parser.add_argument('arguments',nargs='*')
    parser.parse_args()
    import session as c,pilot as p,json,subprocess
    ledgers=sorted([x for x in p.ROOT.rglob('*ledger.json') if '.backups' not in str(x)],key=lambda x:x.stat().st_mtime,reverse=True)
    results=[]
    for ledger in ledgers:
        data=json.loads(ledger.read_text())
        if data.get('restore_status')=='restored':
            results.append(dict(ledger=str(ledger),returncode=0,already_restored=True));p.rec('LEDGER-RESTORATIONS.json',results);continue
        assert data.get('entries') and not data.get('wedged'),('Uncertain/wedged ledger: preserve, do not retry',str(ledger))
        with ledger.with_suffix('.restore.log').open('w') as out:
            result=subprocess.run([c.PY,c.STAGER,'--restore',str(ledger),'--verify-hash','--chunk-size','1048576','--log',str(ledger)+'.restore.ftp.jsonl'],stdout=out,stderr=subprocess.STDOUT)
        results.append(dict(ledger=str(ledger),returncode=result.returncode));p.rec('LEDGER-RESTORATIONS.json',results)
        assert result.returncode==0,('Restore failed',str(ledger))
    print('STAGE LEDGERS RESTORED',len(results),flush=True)

if __name__=='__main__':
    main()
