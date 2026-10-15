#!/usr/bin/env python3
"""Summarize raw survey rows without trimming stalls or converting host timings into Vita claims."""
import argparse,csv,json,statistics,hashlib,re
from pathlib import Path
def stats(v):
    if not v:return None
    v=sorted(v)
    return dict(n=len(v),median=statistics.median(v),p95=v[int(len(v)*.95)],mean=statistics.mean(v),max=v[-1])
def budgets(rows):
    v=[r['wall_us'] for r in rows];longest=run=longest50=run50=0
    previous=None
    for r,n in zip(rows,v):
        epoch=r['file'].split('-light-')[0]
        if epoch!=previous:run=run50=0
        run=run+1 if n>33300 else 0;longest=max(longest,run)
        run50=run50+1 if n>50000 else 0;longest50=max(longest50,run50);previous=epoch
    return dict(above_33_3_fraction=sum(n>33300 for n in v)/len(v),above_50_fraction=sum(n>50000 for n in v)/len(v),longest_above_33_3_frames=longest,longest_above_50_frames=longest50)
def main():
    ap=argparse.ArgumentParser();ap.add_argument('root',type=Path);args=ap.parse_args()
    root=args.root;m=json.loads((root/'effective-manifest.json').read_text());results=json.loads((root/'results.json').read_text())
    summary=dict(source='raw light CSV including export and backend stalls',console_acceptance=False,p95='sorted[floor(n*0.95)]',cells=[])
    for cell in results:
        prefix=cell['scene']+'-r'+str(cell['repeat']);rows=[]
        def order(p):
            m=re.search(r'-e(\d+)-light-',p.name)
            epoch=int(m.group(1)) if m else 1
            end=int(p.stem.rsplit('-',1)[1])
            segment=0 if '-setup-light-' in p.name or '-source-light-' in p.name else 2 if '-destination-light-' in p.name else 1
            return segment,epoch,end
        paths=sorted(root.glob(prefix+'*-light-*.csv'),key=order)
        for p in paths:
            with p.open() as f:
                for raw in csv.DictReader(f):
                    r={k:int(v) for k,v in raw.items()};r['file']=p.name
                    r['render_sum_us']=sum(r[x+'_cpu_us'] for x in ['world_opaque','world_alpha','actors','fx','hud','present'])
                    r['unscoped_cpu_us']=r['frame_cpu_us']-r['render_sum_us']-r['skin_gather_us']-r['skin_collect_wait_us']
                    r['outer_us']=r['wall_us']-r['frame_cpu_us']-r['swap_us']
                    rows.append(r)
        timed=[r for r in rows if r['timed']]
        row=dict(cell=cell,all_frames=len(rows),timed_frames=len(timed),csv_sha256={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in paths})
        if rows:
            # Each epoch resets its sequence; duplicates within one epoch are an error.
            identities=[(r['file'].split('-light-')[0],r['frame']) for r in rows]
            assert len(identities)==len(set(identities)),prefix+' duplicate rows'
            row['timed_metrics']={k:stats([r[k] for r in timed]) for k in timed[0] if k!='file'} if timed else {}
            row['budgets']=budgets(timed) if timed else None
            # Keep independent epochs and transition source/destination visible.
            # A combined cell distribution is descriptive, not a replacement for these.
            row['segments']={}
            for segment in dict.fromkeys(r['file'].split('-light-')[0] for r in rows):
                all_segment=[r for r in rows if r['file'].split('-light-')[0]==segment]
                retained=[r for r in all_segment if r['timed']]
                row['segments'][segment]=dict(all_frames=len(all_segment),timed_frames=len(retained),
                    timed_metrics={k:stats([r[k] for r in retained]) for k in retained[0] if k!='file'} if retained else {},
                    budgets=budgets(retained) if retained else None,
                    export_frame_count=sum(r['export_frame'] for r in all_segment))
            row['negative_outer_rows']=[dict(file=r['file'],frame=r['frame'],outer_us=r['outer_us']) for r in rows if r['outer_us']<0]
            row['export_frame_count']=sum(r['export_frame'] for r in rows)
            if cell['scene'] in ['V0','V0-final']:
                compat=[r for r in rows if 61<=r['frame']<=180]
                row['historical_61_180']={k:stats([r[k] for r in compat]) for k in ['wall_us','frame_cpu_us','swap_us']}
        memory=[]
        milestones=[]
        for p in root.glob(prefix+'-milestones.jsonl'):
            milestones=[json.loads(s) for s in p.read_text().splitlines()]
            memory=[s['memory'] for s in milestones]
        if next(s for s in m['scenes'] if s['id']==cell['scene'])['expected'].get('destination'):
            exits=[s['unix_ms'] for s in milestones if s['scope']=='transition' and s['milestone']=='exit-request']
            presented=[s['unix_ms'] for s in milestones if s['scope']=='transition' and s['milestone']=='destination-first-presented']
            row['transition']=dict(exit_request_unix_ms=exits,destination_first_presented_unix_ms=presented,
                exit_to_first_presented_ms=presented[0]-exits[0] if len(exits)==len(presented)==1 else None,
                clock='system wall-clock milestone milliseconds; no separate controllable-frame timestamp')
        row['memory_sample_extrema']={key:stats([v[key] for v in memory if isinstance(v.get(key),int)]) for key in ['used','committed','free_arena','vgl_ram_free','vgl_vram_free']}
        row['memory_is_continuous_peak_measurement']=False
        summary['cells'].append(row)
    summary['families']={family:{'scene_statuses':{state:sum(c['status']==state for c in results if next(s for s in m['scenes'] if s['id']==c['scene'])['family']==family) for state in ['complete','failed','uncovered']}} for family in ['V0','C1','C4','F4','W','E','B','T']}
    (root/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    print(json.dumps(dict(statuses={s:sum(c['status']==s for c in results) for s in ['complete','failed','uncovered']},frames=sum(c['all_frames'] for c in summary['cells']))))
if __name__=='__main__':main()
