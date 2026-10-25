"""Sampled heap windows, bounded by exact host-log byte offsets."""
import json,re

def readout(run):
    raw=(run/'runtime-log.txt').read_bytes()
    launch=json.loads((run/'LAUNCH.json').read_text())
    armed=json.loads((run/'BENCH-ARMED.json').read_text())
    begin=armed['log_offset']-launch['log_offset']
    assert 0<=begin<=len(raw), 'BENCH-ARMED log boundary is outside collected log'
    marker=b'[lg/frame-total] measured=120'
    assert raw.count(marker)==1, 'Ambiguous measured/export completion boundary'
    end=raw.index(marker);assert begin<=end
    def samples(value):
        rows=[]
        for line in value.decode(errors='replace').splitlines():
            found=re.search(r'(?:newlib_uordblks|heap_uordblks_bytes)["\s:=]+(\d+)',line)
            if found:rows.append(dict(bytes=int(found[1]),line=line))
        return dict(samples=rows,count=len(rows),sampled_max_MiB=max((r['bytes'] for r in rows),default=0)/1048576 if rows else None)
    return dict(benchmark=samples(raw[begin:end]),loading_preparation=samples(raw[:begin]),
        post_export=samples(raw[end:]),whole_run=samples(raw),
        benchmark_boundary='BENCH-ARMED byte offset to [lg/frame-total] measured=120',
        post_export_boundary='frame-total record is emitted after census export; earlier export-internal transients cannot be separated without another marker',
        continuous_peak_measured=False,heap_label_is_post_session_only=True)
