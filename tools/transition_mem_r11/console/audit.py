"""Audit the fixed transition memory pass rule; no performance promotion."""
from pathlib import Path
import argparse,json,re

def validate(text):
    assert not any(s in text.lower() for s in ['memory allocation of','out of memory','panicked','coredump','viewer failed'])
    rows=[json.loads(line.split('] ',1)[1]) for line in text.splitlines() if line.startswith('[lg/transition-memory] ')]
    ends=[r for r in rows if r.get('event')=='end' and r.get('kind')!='quit']
    assert [(r['kind'],r['to']) for r in ends]==[('startup-entry','hedge3'),('native-load','pandemonium'),('chapter-or-console','hedge3')], 'Missing/reordered transition completion'
    for row in ends:
        assert row['success'] is True
        assert 0<row['peak_heap_used']<170*1048576, 'Fixed 170 MiB transition budget exceeded'
        assert row['peak_heap_arena']>0
        for name in (['before','after'] if row['kind']=='startup-entry' else ['before','released','after']):
            assert row[name]['heap_used']>0 and row[name]['heap_arena']>0, 'Missing heap/arena sample'
        assert any(r.get('event')=='begin' and all(r[k]==row[k] for k in ['kind','from','to']) for r in rows)
    return dict(status='PASS',transitions=ends,budget_exclusive_MiB=170,continuous_peak_measured=False)
def main():
    p=argparse.ArgumentParser();p.add_argument('label');a=p.parse_args()
    import pilot as q
    q.B=q.ROOT/a.label
    result=validate((q.B/'runtime-log.txt').read_text())
    collected=json.loads((q.B/'COLLECT.json').read_text());assert collected['runtime_log_sha256']==q.sha((q.B/'runtime-log.txt').read_bytes())
    staged=json.loads((q.B/'STAGE.json').read_text());assert staged['status']=='PASS'
    result.update(synthetic_offline_only=(q.ROOT/'SYNTHETIC-OFFLINE-ONLY').exists())
    q.rec('AUDIT.json',result);print('PASS transition peak budget; sampled native heap')
if __name__=='__main__':main()
