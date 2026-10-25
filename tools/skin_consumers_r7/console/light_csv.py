"""Read light CSVs without turning absent observations into zeros."""
import csv,json
from pathlib import Path
PHASES=('world_opaque','world_alpha','actors','fx','hud','present')
def unavailable(key):
    return key=='frame_cpu_us' or key.startswith(('hud_','present_')) or any(key.endswith('_'+x) for x in ('uniform_gl_calls','texture_gl_binds','state_gl_calls','program_gl_changes','deferred_cpu_us'))
def read(path,legacy_observers=None):
    raw=list(csv.DictReader(Path(path).open()))
    assert raw,'empty CSV'
    rows=[]
    for row in raw:
        mode=row.get('gl_observers',legacy_observers)
        assert mode in ('direct','observed'),'missing gl_observers: declare legacy mode explicitly'
        parsed={}
        for k,v in row.items():
            if k=='gl_observers':continue
            parsed[k]=None if (mode=='direct' and unavailable(k)) or v in ('','NA',None) else int(v)
        parsed['gl_observers']=mode
        assert parsed['wall_us'] is not None and parsed['wall_us']>0
        rows.append(parsed)
    assert len({r['gl_observers'] for r in rows})==1,'observer mode changed within run'
    return rows
def stats(values):
    v=list(values)
    if not v or any(x is None for x in v):return None
    v.sort();return dict(n=len(v),median=v[len(v)//2],p95=v[len(v)*95//100],min=v[0],max=v[-1],sum=sum(v),mean=sum(v)/len(v))
def summarize(rows):
    result={k:stats(r[k] for r in rows) for k in rows[0] if k!='gl_observers'}
    result['gl_observers']=rows[0]['gl_observers']
    phases=[[r.get(p+'_cpu_us') for p in PHASES] for r in rows]
    sums=[None if any(x is None for x in v) else sum(v) for v in phases]
    result['six_render_phases_us']=stats(sums)
    result['residual_us']=stats(None if s is None or r.get('frame_cpu_us') is None else r['frame_cpu_us']-s for r,s in zip(rows,sums))
    result['unavailable_fields']=[k for k,v in result.items() if v is None]
    return result

def protocol_entry(label,protocol_path):
    entries=[v for v in (lambda p:p['sequence']+p.get('optional_sequence',[]))(json.loads(Path(protocol_path).read_text())) if v['label']==label]
    assert len(entries)==1,('Unknown or duplicate protocol label',label)
    entry=entries[0]
    assert entry['gl_observers'] in ('observed','direct'),('Invalid protocol mode',label)
    return entry

def check_observers(label,summary,protocol_path):
    """Fail closed on mode mismatch or missing/inactive observed counters."""
    entry=protocol_entry(label,protocol_path)
    expected=entry['gl_observers']
    assert summary['gl_observers']==expected,('CSV mode differs from protocol',label,summary['gl_observers'],expected)
    medians={}
    assert expected=='direct', 'this experiment requires direct mode'
    for key,value in summary.items():
        if key not in ('frame','wall_us','gl_observers','unavailable_fields'):
            assert value is None, ('unexpected profiler observation',key,value)
    return dict(status='PASS',expected_mode=expected,actual_mode=summary['gl_observers'],counter_medians=medians)

if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description='Import-only support library; --help performs no device action.')
    parser.parse_args()
