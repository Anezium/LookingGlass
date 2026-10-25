import argparse

def main():
    parser=argparse.ArgumentParser(description='Preserve and independently verify the fresh human-restore snapshot')
    parser.add_argument('arguments',nargs='*')
    parser.parse_args()
    import pilot as p,json,re
    inv=json.loads((p.ROOT/'starting-inventories.json').read_text());top=inv[p.R]
    paths=[p.R+n for n,v in top.items() if not v['directory']]+[p.A+'sce_sys/param.sfo']
    for d,rows in inv.items():
     for n,v in rows.items():
      if not v['directory'] and ('/saves/' in d or '/route-saves/' in d or (d==p.R+'frame_profile/' and re.match(r'(hedge3|hatter1|skool2|pandemonium)-(full|fullbench)-(light-|census-180\.json$)',n))):paths.append(d+n)
    for n in ['profile_mode','native-timing-config.json','frame_profile.csv','route-inputs-continue.json','native-timing-review.alice-v3.upload','native-timing-review.json']:
     if p.R+n not in paths:paths.append(p.R+n)
    for mapname in ['hedge3','skool2','hatter1','pandemonium']:
     for mode in ['full','fullbench']:
      for suffix in ['-120.csv','-120-doors.csv']:
       path=p.R+'frame_profile/'+mapname+'-'+mode+'-light'+suffix
       if path not in paths:paths.append(path)
    for mapname in ['hedge3','pandemonium']:
     path=p.R+'frame_profile/'+mapname+'-fullbench-census-180.json'
     if path not in paths:paths.append(path)
    p.rec('baseline-paths.json',paths)
    a=p.snapshot('originals',paths);b=p.snapshot('originals-independent-readbacks',paths)
    assert [(v['path'],v['present'],v.get('sha256')) for v in a]==[(v['path'],v['present'],v.get('sha256')) for v in b]
    p.rec('BASELINE.json',dict(time=p.now(),manifest=a,inventories=inv,SELF='pending fresh vita-stage original backup/cache verification'))
    p.rec('PROFILE-BASELINE.json',dict(status='PASS',manifest=[v for v in a if '/frame_profile/' in v['path']]))
    print('BASELINE PASS',len(a),sum(v.get('bytes',0) for v in a),flush=True)

if __name__=='__main__':
    main()
