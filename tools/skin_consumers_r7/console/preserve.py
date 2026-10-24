"""Fresh r7 inventories and sealed package/source/tooling identity."""
import argparse

def main():
    argparse.ArgumentParser(description=__doc__).parse_args()
    import pilot as p, json, shutil
    package=p.ROOT.parent
    protocol=json.loads((package/'CONSOLE-PROTOCOL.json').read_text())
    arms=sorted({entry['package'] for entry in protocol['sequence']+protocol.get('optional_sequence',[])})
    assert arms==['baseline','baseline-census','skin','skin-census']
    identities={arm:json.loads((package/'packages'/arm/'VERIFICATION.json').read_text()) for arm in arms}
    sealed=json.loads((package/'PACKAGE-IDENTITY.json').read_text())
    engine=sealed['engine_source_revision']
    assert sealed['engine_source_revision']==engine
    assert all(v['source_revision']==engine and v['revision']==engine for v in identities.values())
    assert sealed['branch']=='vita-skin-consumers-r7-20261024'
    assert len(sealed['branch_head'])==40 and all(c in '0123456789abcdef' for c in sealed['branch_head'])
    # Enumerate today's directories instead of importing an r4 session's inventory.
    inv={};pending=[p.R,p.A]
    with p.Session(timeout=30) as session:
        while pending:
            directory=pending.pop(0)
            if directory in inv:continue
            rows=p.listing(session,directory);inv[directory]=rows
            for name,row in rows.items():
                assert name not in ('.','..') and '/' not in name
                if row['directory']:pending.append(directory+name+'/')
    p.B=p.ROOT
    p.rec('starting-inventories.json',inv)
    p.rec('SESSION.json',dict(start=p.now(),evidence=str(p.ROOT),os='Darwin',sole_console_owner=True,
        package='alice-skin-consumers-r7',package_self_sha256={arm:v['self_sha256'] for arm,v in identities.items()},
        engine_source_revision=engine,branch=sealed['branch'],branch_head=sealed['branch_head'],
        driver='driver-v5',initial_state='fresh HOME inspected; no quit needed'))
    shutil.copy2(p.CACHE_INDEX,p.ROOT/'cache-index-before.json')
    print('Fresh r7 inventories',len(inv),'engine',engine,'branch HEAD',sealed['branch_head'],flush=True)

if __name__=='__main__':main()
