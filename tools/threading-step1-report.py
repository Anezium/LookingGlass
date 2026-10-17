#!/usr/bin/env python3
"""Strict worker-pair reduction; primary and GPU-attribution populations stay separate."""
import argparse, importlib.util, json
from pathlib import Path
spec=importlib.util.spec_from_file_location('csv_contract',Path(__file__).with_name('threading-step0-report.py'))
contract=importlib.util.module_from_spec(spec);spec.loader.exec_module(contract)
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('console',type=Path)
p.add_argument('--kind',choices=['primary','attribution'],required=True)
p.add_argument('--out',type=Path,required=True)
a=p.parse_args()
probe=a.kind=='attribution';runs=[]
for name in ['gvillage','hatter1','potears1']:
    for pair in range(1,(1 if probe else 3)+1):
        arms={}
        for arm in ['off','on']:
            files,rows=contract.read_window(a.console/name/f'pair-{pair}'/arm,probe)
            contract.require(all(r['world_prepare_fallbacks']==0 for r in rows),f'{name}/{pair}/{arm}: inline fallback')
            contract.require(all((r['world_prepare_jobs']>0)==(arm=='on') for r in rows),f'{name}/{pair}/{arm}: incorrect worker arm')
            arms[arm]=dict(raw_files=list(map(str,files)),selected_rows=rows,
                columns=contract.summarize_rows(rows))
        delta={k:{q:arms['on']['columns'][k][q]-arms['off']['columns'][k][q] for q in ['median','p95','max','mean']}
            for k in ['wall_us','frame_cpu_us','swap_us']}
        runs.append(dict(map=name,pair=pair,arms=arms,worker_on_minus_off_us=delta))
pooled={}
for name in ['gvillage','hatter1','potears1']:
    pooled[name]={}
    for arm in ['off','on']:
        rows=[row for run in runs if run['map']==name for row in run['arms'][arm]['selected_rows']]
        pooled[name][arm]=contract.summarize_rows(rows)
a.out.write_text(json.dumps(dict(kind=a.kind,sim_span_probe=probe,runs=runs,pooled=pooled,
    stalls_removed=0,p95='sorted[floor(.95*N)]',audio_images_and_provenance_require_log_review=True),indent=2)+'\n')
print('PASS',a.kind,len(runs)*2,'launches; GPU, audio, image and provenance interpretation still required')
