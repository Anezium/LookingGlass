"""Compare two fresh arms; timing acceptance uses wall median only."""
import argparse,json
from pathlib import Path
from light_csv import read,summarize
p=argparse.ArgumentParser();p.add_argument('reference');p.add_argument('candidate');p.add_argument('--legacy-observers',choices=['direct','observed']);p.add_argument('--max-wall-delta-ms',type=float,default=1.0);p.add_argument('--output',type=Path);a=p.parse_args()
x,y=[summarize(read(f,a.legacy_observers)) for f in (a.reference,a.candidate)]
delta=(y['wall_us']['median']-x['wall_us']['median'])/1000
result=dict(reference=x,candidate=y,wall_median_delta_ms=delta,timing_gate='PASS' if delta<=a.max_wall_delta_ms else 'STOP_WALL_REGRESSION',gate_field='wall_us',observer_modes_match=x['gl_observers']==y['gl_observers'],unavailable_note='null means unavailable; CPU and GL fields never drive acceptance')
text=json.dumps(result,indent=2)+'\n'
if a.output:a.output.write_text(text)
print(text,end='')
