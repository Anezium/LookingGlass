import argparse

def main():
    parser=argparse.ArgumentParser(description='LABEL; extract entry observations from a collected host log')
    parser.add_argument('arguments',nargs='*')
    parser.parse_args()
    from pathlib import Path
    import json,re,sys
    B=Path(__file__).resolve().parent;label=sys.argv[1];run=B/label
    t=(run/'runtime-log.txt').read_text(errors='replace')
    camera_lines=[l for l in t.splitlines() if '[lg/load-diagnostic]' in l and 'camera_eye=' in l]
    extract=lambda key,line:re.search(r'\b'+key+r'=Vec3\(([^)]+)\)',line)
    numeric=[]
    for line in camera_lines:
     row={key:[float(x.strip()) for x in extract(key,line).group(1).split(',')] for key in ['camera_eye','camera_target','camera_up','feet'] if extract(key,line)}
     row.update(paused='paused=true' in line,line=line);numeric.append(row)
    from run_health import entry_for_label
    entry=entry_for_label(label)
    record=dict(map=entry['map'],visit_key=entry['visit_key'],startup_camera_observations=numeric,startup_lines=[l for l in t.splitlines() if any(k in l for k in ['[lg/startup] map_load','[lg/startup] restored scene reused','bench: auto save restored','bench: loading auto save','bench: camera frozen'])],world_age='unavailable',complete_NPC_state='unavailable',actual_live_yaw_pitch='unavailable',measured_frame_camera='full-export only; unavailable for light',simulation_identity='not established by map/static view alone')
    (run/'ENTRY-STATE.json').write_text(json.dumps(record,indent=2)+'\n')
    print(json.dumps(record,indent=2))

if __name__=='__main__':
    main()
