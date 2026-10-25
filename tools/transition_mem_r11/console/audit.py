"""Audit the fixed transition memory pass rule; no performance promotion."""
from pathlib import Path
import argparse,json,re
from transition_policy import EXPECTED, check_record, check_text_samples, arena_headroom

def validate(text):
    check_text_samples(text)
    assert not any(s in text.lower() for s in ['memory allocation of','out of memory','panicked','key not found','coredump','viewer failed'])
    rows=[json.loads(line.split('] ',1)[1]) for line in text.splitlines() if line.startswith('[lg/transition-memory] ')]
    ends=[r for r in rows if r.get('event')=='end' and r.get('kind')!='quit']
    assert [(r['kind'],r['to']) for r in ends] in [EXPECTED, EXPECTED+[('death-retry','hedge3')]], 'Missing/reordered transition completion'
    assert [r['from'] for r in ends] == ['menu','hedge3','pandemonium']+(['hedge3'] if len(ends)==4 else []), 'Wrong transition source chain'
    pending=None; descriptions=[]
    for row in rows:
        samples=check_record(row)
        if row['event']=='begin':
            assert pending is None, 'Unfinished or nested transition'
            pending=row
        elif row['event']=='end':
            assert pending and all(pending[k]==row[k] for k in ['kind','from','to']), 'Begin/end identity mismatch'
            pending=None
            descriptions.append(dict(kind=row['kind'],to=row['to'],samples=samples,
                peak_heap_used=row['peak_heap_used'],peak_heap_arena=row['peak_heap_arena']))
        else:raise AssertionError('Unknown transition event')
    assert pending is None, 'Missing end record'
    return dict(status='PASS',transitions=ends,allocator_samples=descriptions,
                budget_exclusive_MiB=170,arena_exclusive_MiB=179,continuous_peak_measured=False,
                **arena_headroom(text, rows))
def main():
    p=argparse.ArgumentParser();p.add_argument('label');a=p.parse_args()
    import pilot as q
    q.B=q.ROOT/a.label
    text=(q.B/'runtime-log.txt').read_text()
    try:result=validate(text)
    except (AssertionError,KeyError,TypeError,ValueError) as error:
        rows=[]
        for line in text.splitlines():
            if line.startswith('[lg/transition-memory] '):
                try:rows.append(json.loads(line.split('] ',1)[1]))
                except ValueError:pass
        try:
            headroom=arena_headroom(text,rows)
            headroom['arena_headroom_label']='FAIL_OR_UNKNOWN'
        except (AssertionError,KeyError,TypeError):headroom=dict(arena_headroom_bytes=None,arena_headroom_label='UNKNOWN_MISSING_EVIDENCE')
        q.rec('AUDIT-FAILURE.json',dict(status='FAIL_OR_UNKNOWN',reason=str(error),**headroom))
        raise
    retry=json.loads((q.B/'RETRY-DECISION.json').read_text())
    assert (retry['status']=='COMPLETED') == (len(result['transitions'])==4), 'Retry decision/evidence mismatch'
    assert retry['status'] in ['COMPLETED','UNAVAILABLE_NO_VERIFIED_SAFE_DEATH_ROUTE']
    result['retry']=retry
    collected=json.loads((q.B/'COLLECT.json').read_text());assert collected['runtime_log_sha256']==q.sha((q.B/'runtime-log.txt').read_bytes())
    staged=json.loads((q.B/'STAGE.json').read_text());assert staged['status']=='PASS'
    result.update(synthetic_offline_only=(q.ROOT/'SYNTHETIC-OFFLINE-ONLY').exists())
    q.rec('AUDIT.json',result);print('PASS transition peak budget; sampled native heap')
if __name__=='__main__':main()
