"""Measured host counts and clocks; no conversion into predicted Vita milliseconds."""
import collections,gzip,json,statistics
from pathlib import Path
P=Path(__file__).resolve().parent
def main():
    report={}
    for tape in ['a4-wchess1-01.lgtape','rw-20261116-wchess1-01.lgtape']:
        path=P/'host'/(tape+'-census')/'stdout.log.gz'
        samples=[];frames=[];alpha=[]
        with gzip.open(path,'rt') as f:
            for line in f:
                for prefix,rows in [('SAMPLES_DRAW ',samples),('SIM_MAPS_FRAME ',frames),('WORLD_ALPHA_DRAW ',alpha)]:
                    if line.startswith(prefix):
                        row=json.loads(line[len(prefix):])
                        if prefix=='SIM_MAPS_FRAME ':row={k:row[k] for k in ['frame','render','spans_ms']}
                        rows.append(row);break
        lo,hi=(700,1180) if tape.startswith('a4') else (796,1966)
        frames=[r for r in frames if lo<=r['frame']<=hi]
        by=collections.defaultdict(list)
        for r in samples:
            family=r['pass']
            if family=='actors' and 'chess' in (r.get('texture_name') or '').lower():family='chess_pieces'
            by[family].append(r)
        n=len(set(r['frame'] for r in samples));assert n==3
        families={}
        for family,rows in by.items():
            transitions=0;previous=None
            for r in rows:
                state=tuple(r[k] for k in ['frame','program','texture','blend_src','blend_dst','depth_write','depth_test'])
                transitions+=state!=previous;previous=state
            families[family]=dict(draws_per_sampled_frame=len(rows)/n,index_vertices_per_sampled_frame=sum(r['triangles']*3 for r in rows)/n,observed_state_transitions_per_frame=transitions/n,textures=dict(collections.Counter(r.get('texture_name') or '<unlabelled>' for r in rows)))
        phase={}
        for name in frames[0]['render']:
            phase[name]={k:statistics.mean(r['render'][name][k] for r in frames) for k in frames[0]['render'][name]}
        spans={name:statistics.mean(r.get('spans_ms',{}).get(name,0) for r in frames) for name in sorted({k for r in frames for k in r.get('spans_ms',{})})}
        report[tape]=dict(window=[lo,hi],frames=len(frames),sampled_frames=n,families=families,render_metric_means=phase,span_means_ms=spans,world_alpha_materials=dict(collections.Counter(image for r in alpha for image in r['images'])),limits='GL queries perturb sampled CPU time. Render clocks are inclusive submission scopes; spans nest. Index vertices are triangle indices, not unique VBO vertices. Chess is texture-name classification. Host proportions do not predict Vita cost.')
    (P/'attribution.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))
if __name__=='__main__':main()
