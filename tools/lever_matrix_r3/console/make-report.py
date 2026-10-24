"""DIAGNOSTIC ONLY: legacy single-pair report. Not gate-deciding; only compare-abba.py decides the ABBA gate."""
import argparse,json
from pathlib import Path
from light_csv import read,summarize
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('A');p.add_argument('B');p.add_argument('--legacy-observers',choices=['direct','observed']);p.add_argument('--max-wall-delta-ms',type=float,default=1.0);p.add_argument('--output',type=Path);a=p.parse_args()
    x,y=[summarize(read(f,a.legacy_observers)) for f in (a.A,a.B)]
    delta=(y['wall_us']['median']-x['wall_us']['median'])/1000
    result=dict(A=x,B=y,wall_median_delta_ms=delta,diagnostic_single_pair_threshold='WITHIN' if delta<=a.max_wall_delta_ms else 'ABOVE',gate_deciding=False,gate_tool='compare-abba.py',gate_field='wall_us',observer_modes_match=x['gl_observers']==y['gl_observers'],unavailable_note='null means unavailable; CPU and GL fields never drive acceptance')
    text=json.dumps(result,indent=2)+'\n'
    if a.output:a.output.write_text(text)
    print(text,end='')

if __name__=='__main__':
    main()
