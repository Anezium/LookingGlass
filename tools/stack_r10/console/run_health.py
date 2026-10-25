"""Pure host log health policy shared by collection and audit; no device imports."""
import re

def health_text(text,A=False):
    reasons=[]
    # Audio gates use the camera-freeze-to-done interval shared by all r8 arms.
    bench=text.split('bench: camera frozen map=',1)[1].split('bench: done 120 frames mode=fullbench',1)[0] if 'bench: camera frozen map=' in text else ''
    heap_text=text
    text=bench
    for line in text.splitlines():
        if '[lg/audio-realtime]' in line:
            vals=dict(re.findall(r'(\w+)=([^ ]+)',line))
            if float(vals.get('producer_mix_p95_ms',0))>=8:reasons.append(dict(kind='producer_mix_p95_ms >=8',line=line))
            for k in ['pcm_starved_blocks','rejected_sinks','dropped_reports']:
                if int(vals.get(k,0))>0:reasons.append(dict(kind=k,line=line))
        if '[lg/audio-music]' in line and re.search(r'underrun_frames=[1-9]',line):reasons.append(dict(kind='music underrun',line=line))
        if '[cpal/vita]' in line and re.search(r'underrun_estimates=[1-9]',line):reasons.append(dict(kind='CPAL underrun',line=line))
    for line in heap_text.splitlines():
        if '[lg/skin] serial fallback' in line or 'afterimage consumer must be declared' in line:reasons.append(dict(kind='skin serial fallback',line=line))
        if any(v in line.lower() for v in ["thread 'main' panicked",'memory allocation of','out of memory','fatal error','segmentation fault','frame profile report failed','viewer failed']):reasons.append(dict(kind='global crash/OOM',line=line))
        for k in ['newlib_uordblks','heap_uordblks_bytes']:
            for v in re.findall(k+r'["\s:=]+(\d+)',line):
                if int(v)>180*1048576:reasons.append(dict(kind='heap >180MiB',line=line))
    return reasons


def arm_for_label(label,protocol=None):
    """Resolve an exact admitted protocol identity before any UI input."""
    import json
    from pathlib import Path
    path=Path(protocol) if protocol else Path(__file__).resolve().parents[1]/'CONSOLE-PROTOCOL.json'
    entries=[entry for entry in (lambda p:p['sequence']+p.get('optional_sequence',[])+p.get('smoke_sequence',[]))(json.loads(path.read_text())) if entry['label']==label]
    assert len(entries)==1, ('Unknown or duplicate protocol label',label)
    arm=entries[0]['arm']
    assert arm in ('A','B'), ('Invalid experimental arm',arm)
    return arm

if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description='Import-only support library; --help performs no device action.')
    parser.parse_args()


def entry_for_label(label, protocol=None):
    import json
    from pathlib import Path
    path=Path(protocol) if protocol else Path(__file__).resolve().parents[1]/'CONSOLE-PROTOCOL.json'
    entries=[entry for entry in (lambda p:p['sequence']+p.get('optional_sequence',[])+p.get('smoke_sequence',[]))(json.loads(path.read_text())) if entry['label']==label]
    assert len(entries)==1, ('Unknown or duplicate protocol label',label)
    entry=entries[0]
    assert entry['arm'] in ('A','B') and entry['config_arm'] in ('baseline', 'baseline-census', 'stack', 'stack-census')
    assert entry['map'] in ('hedge3','pandemonium')
    assert entry['visit_key']==('pandemonium$first' if entry['map']=='pandemonium' else None)
    return entry
