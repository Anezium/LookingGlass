"""Measured host counts and clocks; no conversion into predicted Vita milliseconds."""
import collections,gzip,json,statistics,re
from pathlib import Path
P=Path(__file__).resolve().parent
def main():
    report={}
    for tape in ['a4-wchess1-01.lgtape','rw-20261116-wchess1-01.lgtape']:
        path=P/'host'/(tape+'-census')/'stdout.log.gz'
        samples=[];frames=[];alpha=[];packet_vertices={};poses={};pose_cpu=[];worker_cpu=[];chess_ids=set()
        sample_first=900 if tape.startswith('a4') else 1700
        with gzip.open(path,'rt') as f:
            for line in f:
                if '[lg/pose-census]' in line:
                    fields=dict(re.findall(r'(pose|frame|identity|path|model)=([^\s]+)',line))
                    key=tuple(int(fields[k]) for k in ('frame','identity','pose'))
                    poses[key]=fields
                    if 'chess' in fields.get('path','').lower():chess_ids.add(key[1])
                elif '[lg/pose-cpu]' in line:
                    fields=dict(re.findall(r'(pose|frame|identity|cpu_ns)=(\d+)',line))
                    pose_cpu.append(fields)
                elif '[lg/worker-census]' in line:
                    fields=dict(re.findall(r'(frame|cpu_ns)=(\d+)',line))
                    actor=re.search(r'actor=Some\(ActorId\((\d+)\)\)',line)
                    if actor:worker_cpu.append((int(actor[1]),int(fields['cpu_ns'])))
                for prefix,rows in [('SAMPLES_DRAW ',samples),('SIM_MAPS_FRAME ',frames),('WORLD_ALPHA_DRAW ',alpha)]:
                    if line.startswith(prefix):
                        row=json.loads(line[len(prefix):])
                        if prefix=='SIM_MAPS_FRAME ':
                            if sample_first<=row['frame']<=sample_first+2:
                                packet_vertices[row['frame']]=[p['vertices'] for p in row.get('submitted_packets',[])]
                            row={k:row[k] for k in ['frame','render','spans_ms']}
                        rows.append(row);break
        lo,hi=(700,1180) if tape.startswith('a4') else (796,1966)
        tape_frames=len(frames)
        frames=[r for r in frames if lo<=r['frame']<=hi]
        by=collections.defaultdict(list);transitions_by_family=collections.Counter();previous_by_frame={}
        for r in samples:
            family=r['pass']
            if family=='actors' and 'chess' in (r.get('texture_name') or '').lower():family='chess_pieces'
            by[family].append(r)
            state=tuple(r[k] for k in ['program','texture','blend_src','blend_dst','depth_write','depth_test'])
            transitions_by_family[family]+=state!=previous_by_frame.get(r['frame'])
            previous_by_frame[r['frame']]=state
        n=len(set(r['frame'] for r in samples));assert n==3
        vertex_totals=collections.Counter()
        for frame,vertices in packet_vertices.items():
            draws=[r for r in samples if r['frame']==frame and r['pass'] in ('actors','fx')]
            assert len(vertices)==len(draws),(frame,len(vertices),len(draws))
            for draw,count in zip(draws,vertices):
                family=draw['pass']
                if family=='actors' and 'chess' in (draw.get('texture_name') or '').lower():family='chess_pieces'
                vertex_totals[family]+=count
        families={}
        for family,rows in by.items():
            transitions=transitions_by_family[family]
            families[family]=dict(draws_per_sampled_frame=len(rows)/n,index_vertices_per_sampled_frame=sum(r['triangles']*3 for r in rows)/n,observed_state_transitions_per_frame=transitions/n,textures=dict(collections.Counter(r.get('texture_name') or '<unlabelled>' for r in rows)))
            families[family]['packet_vertices_per_sampled_frame']=vertex_totals[family]/n if family in vertex_totals else None
        phase={}
        for name in (frames[0]['render'] or {}):
            phase[name]={k:statistics.mean(r['render'][name][k] for r in frames) for k in frames[0]['render'][name]}
        spans={name:statistics.mean(r.get('spans_ms',{}).get(name,0) for r in frames) for name in sorted({k for r in frames for k in r.get('spans_ms',{})})}
        report[tape]=dict(window=[lo,hi],frames=len(frames),sampled_frames=n,families=families,render_metric_means=phase,render_metrics_available=bool(phase),span_means_ms=spans,world_alpha_materials=dict(collections.Counter(image for r in alpha for image in r['images'])),limits='frame_total_only disables detailed render metrics; absent metrics are unavailable, not zero. GL queries perturb sampled CPU time. Render clocks are inclusive submission scopes; spans nest. Index vertices are triangle indices, not unique VBO vertices. Chess is texture-name classification. Host proportions do not predict Vita cost.')
        chess_pose_ns=sum(int(r['cpu_ns']) for r in pose_cpu if 'chess' in poses.get(tuple(int(r[k]) for k in ('frame','identity','pose')),{}).get('path','').lower())
        report[tape]['chess_cpu_inventory']=dict(body_window=[300,360],body_frames=61,mean_inclusive_body_invocation_ms=chess_pose_ns/61/1e6,worker_window='FULL_TAPE_KNOWN_ACTORS',worker_tape_frames=tape_frames,mean_known_actor_worker_service_ms=sum(ns for actor,ns in worker_cpu if actor in chess_ids)/tape_frames/1e6,actor_ids=sorted(chess_ids),limits='Body invocation is a separate early inventory window and includes attachments. Worker lines lack frame IDs: service is averaged over the whole tape for actors identified by the inventory. Worker service is not main-thread wait and must not be added to main-thread clocks.')
    (P/'attribution.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))
if __name__=='__main__':main()
