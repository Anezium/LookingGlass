"""Offline analysis of final validated per-arm logs; selected windows only."""
import json,math,re,statistics
from pathlib import Path
P=Path(__file__).resolve().parent
def main():
    manifest=json.loads((P/'READY.json').read_text());results=json.loads((P/'wchess1-step-results.json').read_text())
    output=[]
    for result in results:
        arm=next(a for a in manifest['arms'] if a['name']==result['arm'])
        text=Path(result['arm_log']).read_text();rows=[]
        for line in text.splitlines():
            if '[lg/spike] frame=' in line:
                values=dict(re.findall(r'\b(frame|ms|sim|render|hud|present)=([^\s]+)',line))
                if arm['window'][0]<=int(values['frame'])<=arm['window'][1]:rows.append(values)
        assert len(rows)==481
        stats={}
        for key in ['ms','sim','render','hud','present']:
            values=sorted(float(r[key]) for r in rows);assert all(math.isfinite(v) and v>=0 for v in values)
            stats[key]=dict(n=len(values),median=statistics.median(values),p95=values[math.ceil(.95*len(values))-1],worst=max(values))
        output.append(dict(arm=arm['name'],tape=arm['tape'],window=arm['window'],stats=stats))
    (P/'analysis.json').write_text(json.dumps(dict(scope='SELECTED_WINDOWS_ONLY',arms=output),indent=2)+'\n')
    print(json.dumps(output,indent=2))
if __name__=='__main__':main()
