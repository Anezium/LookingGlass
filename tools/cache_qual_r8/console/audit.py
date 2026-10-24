"""Offline gameplay qualification audit. Hit rates and timings are descriptive."""
from pathlib import Path
import argparse, json, math, re, statistics
from run_health import audio, window_text, entry_for_label, health_text
MIB=1048576
LABELS=['hedge3-move','hedge3-check','pandemonium-move','pandemonium-check','reload-post','transition-post']
ENTRY_LABELS={'hedge3-move','pandemonium-move','reload-post','transition-post'}
def movement(rows):
    moving_us=0;path=0.;frames=0
    for a,b in zip(rows,rows[1:]):
        dt=b['us']-a['us'];assert 0<dt<=1_000_000,'gameplay discontinuity'
        distance=math.dist(a['eye'],b['eye']);assert math.isfinite(distance)
        path+=distance
        if distance>.001:moving_us+=dt;frames+=1
    return moving_us,path,frames
def pulse_diagnostics(data,motion):
    result={}
    for w in data['windows']:
        if not w['label'].endswith('-move'):continue
        rows=w['rows'];pulses=motion[w['label']]['pulses'];first=pulses[0]['host_start_us']
        intervals=[(a['us']-rows[0]['us'],b['us']-rows[0]['us'],math.dist(a['eye'],b['eye'])>.001)
                   for a,b in zip(rows,rows[1:])] if rows else []
        details=[]
        for pulse in pulses:
            start=pulse['host_start_us']-first;end=pulse['host_end_us']-first
            moved=sum(max(0,min(b,end)-max(a,start)) for a,b,moving in intervals if moving)
            sampled=sum(max(0,min(b,end)-max(a,start)) for a,b,_ in intervals)
            details.append(dict(**pulse,estimated_moving_us=moved,estimated_stationary_us=sampled-moved,
                rpc_envelope_us=end-start,rpc_excess_over_hold_us=max(0,end-start-pulse['seconds']*1_000_000)))
        result[w['label']]=dict(pulses=details,native_moving_us=sum(max(0,b-a) for a,b,moving in intervals if moving),
            alignment='DESCRIPTIVE estimate: first native row aligned to first host pulse request; clocks are not synchronized')
    return result
def stats(values):
    assert values
    a=sorted(values)
    return dict(count=len(a),mean=statistics.mean(a),median=a[len(a)//2],p95=a[min(len(a)-1,int(len(a)*.95))],maximum=a[-1])
def validate(data,text,arm):
    assert data['schema']==1 and data['capacity']==8192
    assert not data.get('heap_failed',False),'native heap limit exceeded'
    assert data['query_classes']==['room','ambient','effects']
    windows=data['windows'];assert [w['label'] for w in windows]==LABELS
    assert all(not w['overflow'] and w['rows'] and w['end_us']>w['rows'][-1]['us'] for w in windows)
    assert 'Viewer closed cleanly.' in text and not health_text(text)
    result=[]
    for w in windows:
        label=w['label'];rows=w['rows'];times=[r['us'] for r in rows]
        assert all(b>a for a,b in zip(times,times[1:]))
        assert len({r['scene'] for r in rows})==1 and rows[0]['scene']!=0
        expected='hedge3' if label.startswith('hedge3') or label=='transition-post' else 'pandemonium'
        assert w['map']==expected
        assert all(r['prepare_ns']>0 and 0<r['heap']<=180*MIB for r in rows)
        assert all(len(r['queries'])==3 and len(r['cache'])==3 for r in rows)
        assert all(r['mismatches']==0 and r['checks']>=0 for r in rows),'self-check mismatch'
        for r in rows:
            assert all(r['queries'][i]==sum(r['cache'][i][:2]) for i in range(3)),'query accounting'
            assert all(all(isinstance(n,int) and n>=0 for n in c) and c[2]<=c[1] for c in r['cache'])
            if arm=='A':assert all(c[0]==c[2]==0 for c in r['cache']) and r['checks']==0
        moving=label.endswith('-move')
        if moving:
            assert w['check_stride']==0,'self-check must be off during movement measurements'
            assert times[-1]-times[0]>=60_000_000,'less than 60 seconds of actual gameplay rows'
            moving_us,path,moving_frames=movement(rows)
            assert moving_us>=60_000_000 and path>=64,'listener did not move for 60 seconds'
        else:
            moving_us=path=moving_frames=None
            assert times[-1]-times[0]>=3_000_000
        checks=sum(r['checks'] for r in rows)
        if label.endswith('-check'):
            assert w['check_stride']==64
            if arm=='B':assert checks>0,'no sampled cached hits checked'
        elif label in {'reload-post','transition-post'}:
            assert w['check_stride']==64
            if arm=='B':assert checks>0,'no post-load sampled cached hits checked'
        else:assert w['check_stride']==0 and checks==0
        counts=[[sum(r['cache'][i][j] for r in rows) for j in range(3)] for i in range(3)]
        health=audio(window_text(text,label));assert health['status']=='PASS',health
        result.append(dict(label=label,map=w['map'],scene=rows[0]['scene'],duration_us=times[-1]-times[0],
            moving_us=moving_us,listener_path_units=path,moving_frames=moving_frames,first_eye=rows[0]['eye'],last_eye=rows[-1]['eye'],
            prepare_acoustics_ms=stats([r['prepare_ns']/1e6 for r in rows]),heap_bytes=stats([r['heap'] for r in rows]),
            per_class={name:dict(hit=c[0],miss=c[1],eviction=c[2],hit_rate=c[0]/sum(c[:2]) if sum(c[:2]) else None)
                       for name,c in zip(data['query_classes'],counts)},checks=checks,mismatches=0,audio=health))
    # Include the transitions, reloads and intervening menus in the health gate;
    # preserve loading before first start and shutdown after last stop separately.
    begin=text.index('[lg/cache-qual] start ')
    end=text.rindex('[lg/cache-qual] stop ')
    session_audio=audio(text[begin:end]);assert session_audio['status']=='PASS',session_audio
    events=data['events'];loads=[e for e in events if e['kind']=='load']
    assert len(loads)==text.count('[lg/audio-load] level='),'missing load telemetry'
    transitions=[e for e in events if e['kind']=='transition']
    assert len(transitions)==2*text.count('[lg/transition-memory] '),'missing transition telemetry'
    before=[e for e in transitions if e['stage']=='before-drop']
    after=[e for e in transitions if e['stage']=='after-drop']
    assert len(before)==len(after)>=3
    cross=[e for e in before if e['operation']=='save-reload' and e['from']!=e['to']]
    reload=[e for e in before if e['operation']=='save-reload' and e['from']==e['to']=='pandemonium']
    chapter=[e for e in before if e['operation']=='chapter-or-console' and e['to']=='hedge3']
    assert cross,'no cross-map native Auto load'
    assert reload,'no same-map native reload'
    assert any(x['us']<r['us']<c['us'] for x in cross for r in reload for c in chapter),'native load/reload/chapter order'
    assert all(e['heap']>0 for e in events),'invalid event heap'
    # Audio-load reset occupancy is sampled at the same load boundary.
    assert all(e['heap']<=180*MIB for e in events if e['kind'] not in {'transition','load'}
               and not (e['kind']=='reset' and e['reason']=='audio-load')),'non-load reset heap budget'
    resets=[e for e in events if e['kind']=='reset']
    if arm=='B':
        assert all(e['entries_after']==0 and e['pin_after'] is False for e in resets),'cache not empty after reset'
        load_resets=[e for e in resets if e['reason']=='audio-load']
        assert len(load_resets)==len(loads)
        for t in before:
            a=next(e for e in after if e['us']>=t['us'])
            load=next(e for e in loads if e['us']>=a['us'])
            reset=next(e for e in load_resets if e['us']>=a['us'])
            assert t['us']<=a['us']<=reset['us']<=load['us']
            prior=[w for w in windows if w['rows'][-1]['us']<t['us']]
            # Initial chapter entry may be cold. Every operation after gameplay
            # must prove it replaced a warm cache and released its last old pin.
            if prior:
                assert reset['entries_before']>0 and reset['old_refs_before']==1
                assert reset['old_scene']==prior[-1]['rows'][-1]['scene']
                following=next(w for w in windows if w['rows'][0]['us']>load['us'])
                assert following['rows'][0]['scene']!=reset['old_scene']
        assert sum(e['entries_before']>0 for e in load_resets)>=3
    else:assert not resets
    return dict(status='PASS',arm=arm,windows=result,events=events,audio_session=session_audio,
        outside_measurement_audio=dict(loading=audio(text[:begin],False),shutdown=audio(text[end:],False)),
        runtime_audio_memory=[line for line in text.splitlines() if line.startswith('[lg/audio-memory] ') and 'event=runtime:' in line],
        event_heap_rule='transition/load and audio-load reset absolute heap descriptive; paired transition/load <=+1 MiB; gameplay <=180 MiB',
        hit_rate_rule='DESCRIPTIVE',performance_rule='default-on also requires each movement B-A mean <=+0.25 ms; otherwise written human acceptance')
def audit(root,label):
    run=root/label;entry=entry_for_label(label,root.parent/'CONSOLE-PROTOCOL.json')
    assert json.loads((run/'LOG-INTEGRITY.json').read_text())['status']=='PASS'
    receipt=json.loads((run/'COLLECT.json').read_text())['files'][0]
    import hashlib
    raw=Path(receipt['local']).read_bytes();assert hashlib.sha256(raw).hexdigest()==receipt['sha256']
    text=(run/'runtime-log.txt').read_text()
    configs=[json.loads(line.split(' config ',1)[1]) for line in text.splitlines() if line.startswith('[lg/vita-renderer] config ')]
    expected=json.loads((root.parent/'configs'/f"PL-{entry['package']}.json").read_text())
    assert configs and all({k:c.get(k) for k in expected}==expected for c in configs),'effective config mismatch'
    assert all(c['audio_cache_qualification'] and not c['phase_census'] and not c.get('audio_offmain',False) and c['audio_static_cache']==(entry['arm']=='B') for c in configs)
    data=json.loads(raw)
    motion=json.loads((run/'MOTION.json').read_text())
    assert set(motion)=={'hedge3-move','pandemonium-move'}
    protocol=json.loads((root.parent/'CONSOLE-PROTOCOL.json').read_text())
    for label,v in motion.items():
        axis=protocol['motion']['per_map'][label.removesuffix('-move')]
        assert v['sequence']==axis['sequence'] and v['status']=='PASS' and len(v['pulses'])==32
    diagnostics=pulse_diagnostics(data,motion)
    try:result=validate(data,text,entry['arm'])
    except AssertionError as error:
        (run/'AUDIT.json').write_text(json.dumps(dict(status='FAIL',reason=str(error),per_pulse_motion=diagnostics),indent=2)+'\n')
        raise
    result['per_pulse_motion']=diagnostics
    result['motion']=motion
    (run/'AUDIT.json').write_text(json.dumps(result,indent=2)+'\n')
    return result
def report(root):
    seq=json.loads((root.parent/'CONSOLE-PROTOCOL.json').read_text())['sequence']
    results=[json.loads((root/e['label']/'AUDIT.json').read_text()) for e in seq]
    assert [r['arm'] for r in results]==['A','B'] and all(r['status']=='PASS' for r in results)
    a,b=results;comparisons=[];unknown=[];performance=[]
    for wa,wb in zip(a['windows'],b['windows']):
        assert wa['label']==wb['label'] and wa['map']==wb['map']
        first_distance=math.dist(wa['first_eye'],wb['first_eye'])
        if wa['label'] in ENTRY_LABELS:assert first_distance<=1,'entry/listener pose mismatch'
        delta=wb['heap_bytes']['maximum']-wa['heap_bytes']['maximum']
        runtime_differs=a['runtime_audio_memory']!=b['runtime_audio_memory']
        noisy=wa['label'].endswith('-move') and delta>MIB and runtime_differs
        if noisy:unknown.append(wa['label'])
        else:assert delta<=MIB,('window heap delta',wa['label'],delta)
        mean=wb['prepare_acoustics_ms']['mean']-wa['prepare_acoustics_ms']['mean']
        if wa['label'].endswith('-move'):performance.append(mean<=.25)
        comparisons.append(dict(label=wa['label'],B_minus_A_heap_bytes=delta,
            heap_status='UNKNOWN_RUNTIME_AUDIO_MEMORY_DIFFERS' if noisy else 'PASS',
            first_eye_A=wa['first_eye'],first_eye_B=wb['first_eye'],last_eye_A=wa['last_eye'],last_eye_B=wb['last_eye'],
            first_eye_AB_distance=first_distance,last_eye_AB_distance=math.dist(wa['last_eye'],wb['last_eye']),
            pose_rule='GATED' if wa['label'] in ENTRY_LABELS else 'DESCRIPTIVE',
            B_minus_A_prepare_mean_ms=mean,
            B_minus_A_prepare_p95_ms=wb['prepare_acoustics_ms']['p95']-wa['prepare_acoustics_ms']['p95']))
    ea=[e for e in a['events'] if e['kind'] in ['transition','load']]
    eb=[e for e in b['events'] if e['kind'] in ['transition','load']]
    assert len(ea)==len(eb)
    for x,y in zip(ea,eb):
        assert {k:v for k,v in x.items() if k not in ['heap','us','cache_on']}=={k:v for k,v in y.items() if k not in ['heap','us','cache_on']}
        assert y['heap']-x['heap']<=MIB,('transition/load heap delta',x,y)
    result=dict(status='UNKNOWN_MATCHED_PAIR_REQUIRED' if unknown else 'PASS_GAMEPLAY_QUALIFICATION_PENDING_HUMAN_REVIEW',default_on=False,
                unknown_movement_heap=unknown,new_pair_requires_new_amendment=bool(unknown),
                default_on_performance_eligible=all(performance) and not unknown,
                default_on_rule='each movement B-A mean <=+0.25 ms; otherwise written human acceptance of moving cost against r6 idle gain',
                arms=results,comparisons=comparisons,reading_rules='sealed before console data',
                BENCHMARK_LATE_CALLBACKS={r['arm']:r['audio_session']['BENCHMARK_LATE_CALLBACKS'] for r in results})
    (root/'RESULTS.json').write_text(json.dumps(result,indent=2)+'\n')
    return result
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,default=Path(__file__).resolve().parent);p.add_argument('label');a=p.parse_args()
    r=report(a.root) if a.label=='report' else audit(a.root,a.label)
    print(r['status'])
