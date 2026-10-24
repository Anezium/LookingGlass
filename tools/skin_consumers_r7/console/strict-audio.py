from pathlib import Path
import json,re,sys,math
import argparse
from run_health import arm_for_label
def main():
    p=argparse.ArgumentParser();p.add_argument('label');p.add_argument('--root',type=Path,required=True);p.add_argument('--protocol',type=Path);a=p.parse_args()
    r=a.root
    label=a.label;arm=arm_for_label(label,a.protocol);run=r/label;t=(run/'runtime-log.txt').read_text(errors='replace')
    start='bench: camera frozen map=';end='bench: done 120 frames mode=fullbench'
    complete=t.count(start)==1 and t.count(end)==1
    before,rest=t.split(start,1) if start in t else (t,'')
    bench,after=rest.split(end,1) if end in rest else (rest,'')
    required={'realtime':('lg/audio-realtime',['window_ms','producer_blocks','producer_mix_p95_ms','pcm_starved_blocks','rejected_sinks','dropped_reports','queued_samples','producer_gap_max_ms','producer_sleep_overshoot_max_ms']), 'music':('lg/audio-music',['decode_ms','underrun_frames','chunks']), 'cpal':('cpal/vita',['late_callbacks','underrun_estimates','callbacks','callback_p95_us','callback_max_us'])}
    def parse(text):
     out={};unknown=[]
     for group,(tag,keys) in required.items():
      rows=[]
      for line in text.splitlines():
       if '['+tag+']' not in line:continue
       # Startup messages are retained elsewhere; these are reporter records only.
       if not any(k+'=' in line for k in keys[:1]):continue
       vals=dict(re.findall(r'(\w+)=([^\s]+)',line));issues=[]
       if len(re.findall(r'\[(?:lg/|cpal/)',line))!=1:issues.append('interleaved log tags')
       for k in keys:
        try:assert math.isfinite(float(vals[k]))
        except (KeyError,ValueError,AssertionError):issues.append('missing/corrupt '+k)
       if issues:unknown.append(dict(group=group,issues=issues,line=line))
       rows.append(dict(fields=vals,raw=line,complete=not issues))
      out[group]=rows
     return dict(window_counts={k:len(v) for k,v in out.items()},raw=out,unknown=unknown)
    windows={name:parse(text) for name,text in [('loading_entry',before),('benchmark',bench),('shutdown',after)]}
    b=windows['benchmark'];reasons=[]
    if not complete:reasons.append('missing/ambiguous benchmark bounds')
    if b['unknown'] or any(v==0 for v in b['window_counts'].values()):reasons.append('UNKNOWN missing/corrupt/interleaved benchmark audio fields')
    for group,(tag,keys) in required.items():
     if bench.count('['+tag+']')!=b['window_counts'][group]:reasons.append('UNKNOWN unparsed benchmark '+group+' tag')
    for group,key in [('realtime','producer_blocks'),('cpal','callbacks')]:
     if any(row['complete'] and float(row['fields'][key])<=0 for row in b['raw'][group]):reasons.append('UNKNOWN nonpositive benchmark '+key)
    limits={'realtime':['pcm_starved_blocks','rejected_sinks','dropped_reports'],'music':['underrun_frames'],'cpal':['underrun_estimates','late_callbacks']}
    for group,keys in limits.items():
     for row in b['raw'][group]:
      if row['complete']:
       for k in keys:
        if float(row['fields'][k])!=0:reasons.append(group+' '+k+' nonzero')
    p95=[float(x['fields']['producer_mix_p95_ms']) for x in b['raw']['realtime'] if x['complete']]
    if any(v>=8 for v in p95):reasons.append('merged benchmark producer p95 >=8ms')
    result=dict(status='FAIL_OR_UNKNOWN' if reasons else 'PASS',reasons=reasons,A_p95_record_only=False,benchmark_producer_p95_ms=p95,max_reported_benchmark_producer_p95_ms=max(p95,default=None),windows=windows,counter_semantics='Raw reporter fields preserved. PCM starvation and music underruns are per reporter window; dropped_reports and CPAL fields retained as reported, no cumulative-field pooling. Intentional flush silence is never subtracted.',listening='UNAVAILABLE')
    result['benchmark_raw_counter_values']={g:{k:[row['fields'].get(k) for row in b['raw'][g]] for k in keys} for g,keys in limits.items()}
    result['benchmark_observed_cumulative_deltas']={}
    for g,k in [('realtime','dropped_reports'),('cpal','underrun_estimates'),('cpal','late_callbacks')]:
     values=[int(row['fields'][k]) for row in b['raw'][g] if row['complete']]
     result['benchmark_observed_cumulative_deltas'][g+'.'+k]=dict(raw_values=values,last_minus_first=values[-1]-values[0] if values else None,note='Observed in-slice endpoint delta only; raw values independently gate zero, never inferred from delta.')
    (run/'STRICT-AUDIO.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k!='windows'},indent=2))
    sys.exit(2 if reasons else 0)

if __name__=='__main__':
    main()
