import argparse

def main():
    parser=argparse.ArgumentParser(description='Preserve fresh session inventories and package identities')
    parser.add_argument('arguments',nargs='*')
    parser.parse_args()
    import pilot as p, json, shutil
    old=p.Path('/Users/anezium/vita-j0/resume-2026-10-22/alice-card-think/console-r4/starting-inventories.json')
    with p.Session(timeout=30) as s:
        inv={d:p.listing(s,d) for d in json.loads(old.read_text())}
    p.rec('starting-inventories.json',inv)
    p.rec('SESSION.json',dict(start=p.now(),evidence=str(p.ROOT),os='Darwin',sole_console_owner=True,package_self_sha256={arm:json.loads((p.ROOT.parent/'packages'/arm/'VERIFICATION.json').read_text())['self_sha256'] for arm in ['baseline','world','fx','audio']},driver='driver-v5',initial_state='fresh HOME inspected; no quit needed'))
    shutil.copy2('/Users/anezium/vita-j0/cache/vita-stage-index.json',p.ROOT/'cache-index-before.json')
    print('Fresh inventories',len(inv),flush=True)

if __name__=='__main__':
    main()
