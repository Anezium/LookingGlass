"""Pure host qualification policy. No device imports."""
import json, math, re
from pathlib import Path
from transition_policy import check_record, check_text_samples
REQUIRED={
 'lg/audio-realtime':['window_ms','producer_blocks','producer_mix_p95_ms','pcm_starved_blocks','rejected_sinks','dropped_reports','queued_samples','producer_gap_max_ms','producer_sleep_overshoot_max_ms'],
 'lg/audio-music':['decode_ms','underrun_frames','chunks'],
 'cpal/vita':['late_callbacks','underrun_estimates','callbacks','callback_p95_us','callback_max_us']}
def entry_for_label(label,protocol=None):
    p=Path(protocol) if protocol else Path(__file__).resolve().parents[1]/'CONSOLE-PROTOCOL.json'
    rows=[e for e in json.loads(p.read_text())['sequence'] if e['label']==label]
    assert len(rows)==1,('Unknown protocol identity',label)
    return rows[0]
def arm_for_label(label,protocol=None):return entry_for_label(label,protocol)['arm']
def audio(text,required=True):
    raw={k:[] for k in REQUIRED};reasons=[]
    for line in text.splitlines():
        for tag,keys in REQUIRED.items():
            if '['+tag+']' not in line:continue
            values=dict(re.findall(r'(\w+)=([^\s]+)',line))
            # Outside a window startup records are descriptive, never inferred healthy.
            try:
                vals={k:float(values[k]) for k in keys}
                assert all(math.isfinite(v) and v>=0 for v in vals.values())
            except (KeyError,ValueError,AssertionError):
                reasons.append('missing/corrupt '+tag);raw[tag].append(dict(raw=line,fields=values,complete=False));continue
            raw[tag].append(dict(raw=line,fields=values,complete=True))
            bad={'lg/audio-realtime':['pcm_starved_blocks','rejected_sinks','dropped_reports'],
                 'lg/audio-music':['underrun_frames'],'cpal/vita':['underrun_estimates']}[tag]
            reasons += [tag+' '+k for k in bad if vals[k]!=0]
            if tag=='lg/audio-realtime' and (vals['producer_mix_p95_ms']>=8 or vals['producer_blocks']<=0):reasons.append('producer p95/count')
            if tag=='cpal/vita' and vals['callbacks']<=0:reasons.append('callback count')
    if required and any(not v for v in raw.values()):reasons.append('missing audio reporter group')
    late=[int(r['fields']['late_callbacks']) for r in raw['cpal/vita'] if r['complete']]
    return dict(status='FAIL_OR_UNKNOWN' if reasons else 'PASS',reasons=reasons,raw=raw,
                BENCHMARK_LATE_CALLBACKS=dict(per_window=late,total=sum(late),report_only=True))
def window_text(text,label):
    start=re.findall(r'^\[lg/cache-qual\] start id=\d+ label='+re.escape(label)+r' check_stride=\d+$',text,re.M)
    stop=re.findall(r'^\[lg/cache-qual\] stop id=\d+ label='+re.escape(label)+r'$',text,re.M)
    assert len(start)==len(stop)==1,('ambiguous window',label)
    return text.split(start[0],1)[1].split(stop[0],1)[0]
def health_text(text,A=False):
    reasons=[]
    try:check_text_samples(text)
    except AssertionError as error:reasons.append(str(error))
    if any(s in text.lower() for s in ["thread 'main' panicked",'out of memory','memory allocation of','fatal error','segmentation fault','viewer failed','[lg/cache-qual] error']):reasons.append('global failure')
    # Passive live stop checks use only opt-in windows, including check windows.
    lines=[]
    if '[lg/cache-qual] start ' in text:
        interval=text.split('[lg/cache-qual] start ',1)[1]
        final=re.search(r'^\[lg/cache-qual\] stop id=\d+ label=transition-post$',interval,re.M)
        if final:interval=interval[:final.start()]
        lines=interval.splitlines()
    result=audio('\n'.join(lines),False)
    # Partial reporter writes are checked strictly after collection, not replayed.
    reasons += [r for r in result['reasons'] if not r.startswith('missing/corrupt')]
    # Absolute heap at loads/transitions is descriptive (R1). Native recorder
    # heap errors and collected gameplay rows remain the absolute stop.
    for n in re.findall(r'^\[lg/cache-qual\].*heap_uordblks_bytes[\s:=]+(\d+)',text,re.M):
        if int(n)>180*1048576:reasons.append('heap >180MiB')
    for line in text.splitlines():
        if line.startswith('[lg/transition-memory] '):
            row=json.loads(line.split('] ',1)[1])
            try:check_record(row)
            except (AssertionError,TypeError,KeyError) as error:reasons.append(str(error))
    return reasons
