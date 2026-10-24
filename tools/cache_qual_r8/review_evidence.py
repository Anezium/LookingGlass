"""Read retained local r6 images and r5/r6 Auto-load logs; no transport imports."""
from pathlib import Path
import argparse, hashlib, json, re

BASE=Path('/Users/anezium/vita-j0/resume-2026-10-24')
SESSIONS={'r5':BASE/'alice-phase-census-r5/session-s6-phase-census',
          'r6':BASE/'alice-acoustic-cache-r6/session-s7-acoustic-cache'}

def evidence():
    loads=[]
    for revision,root in SESSIONS.items():
        for path in sorted(root.glob('*/runtime-log.txt')):
            lines=path.read_text().splitlines()
            starts=[i for i,line in enumerate(lines) if '[lg/startup] save_restore begin ' in line]
            for start in starts:
                end=next(i for i in range(start+1,len(lines)) if '[lg/startup] restore_resources end ' in lines[i])
                # Include one reporter on each side: the first after completion
                # can cover the last second of the load.
                reporters=[i for i,line in enumerate(lines) if line.startswith('[lg/audio-realtime] ')]
                selected=[i for i in reporters if start<=i<=end]
                selected += [max(i for i in reporters if i<start),min(i for i in reporters if i>end)]
                rows=[]
                for i in sorted(selected):
                    fields=dict(re.findall(r'(\w+)=([^\s]+)',lines[i]))
                    rows.append(dict(line=i+1,pcm_starved_blocks=int(fields['pcm_starved_blocks']),
                                     producer_p95_ms=float(fields['producer_mix_p95_ms'])))
                assert any('restored scene reused map=pandemonium' in line for line in lines[start:end+1])
                loads.append(dict(revision=revision,path=str(path),sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                    start_line=start+1,end_line=end+1,reporters=rows,
                    pcm_starved_blocks=sum(r['pcm_starved_blocks'] for r in rows),
                    maximum_producer_p95_ms=max(r['producer_p95_ms'] for r in rows),
                    p95_at_least_8ms=sum(r['producer_p95_ms']>=8 for r in rows)))
    images=[]
    for name in ['s7-acoustic-cache-hedge3-1-A/captures/begin.png',
                 's7-acoustic-cache-hedge3-1-A/captures/paused-overlay.png',
                 's7-acoustic-cache-pandemonium-1-A/captures/entry-pose.png']:
        path=SESSIONS['r6']/name
        images.append(dict(path=str(path),sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
    return dict(status='PASS',offline_only=True,device_contact=False,driver_contact=False,
                method='save_restore begin through restore_resources end, plus one audio reporter on each side',
                loads=loads,images=images)

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    result=evidence();args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    for revision in SESSIONS:
        rows=[r for r in result['loads'] if r['revision']==revision]
        print(revision,'native Auto loads',len(rows),'starved blocks',sum(r['pcm_starved_blocks'] for r in rows),
              'max producer p95 ms',max(r['maximum_producer_p95_ms'] for r in rows),
              'p95 >=8ms reporters',sum(r['p95_at_least_8ms'] for r in rows))
