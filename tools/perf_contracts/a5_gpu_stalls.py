#!/usr/bin/env python3
"""Print exact EndScene rows 2100..2130 from marked rc1kit logs; no device access."""
import argparse,json,re
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('logs',nargs='+',help='LOG or ARM=LOG');p.add_argument('--json',action='store_true');a=p.parse_args()
rows=[]
for item in a.logs:
 label,sep,path=item.partition('=')
 if not sep:path=item;label=Path(path).stem
 run=0;mark=label
 for line in Path(path).read_text(errors='replace').splitlines():
  if '[lg/vita-renderer] config ' in line and not sep:
   try:
    config=json.JSONDecoder().raw_decode(line.split('[lg/vita-renderer] config ',1)[1])[0]
    cuts=[key.removeprefix('diag_skip_').replace('_','-') for key,on in config.items() if key.startswith('diag_skip_') and on]
    if cuts:mark='+'.join(cuts)
    elif config.get('sim_flight_memo'):mark='route-any' if config.get('sim_sweep_any') else 'route'
    else:mark='spike' if config.get('spike_log_ms',0)>0 else 'control'
   except (ValueError,TypeError):pass
  # Receiver marks are retained in many rc1kit log exports. A new frame=1 isn't
  # required: this logger is deliberately bounded to the fight window.
  m=re.search(r'\[lg/replay\] (?:play|playing|start)\b',line)
  if m:run+=1
  m=re.search(r'\[lg/spike\] frame=(\d+)\b(.*)',line)
  if not m:continue
  frame=int(m[1])
  if not 2100<=frame<=2130:continue
  fields=dict(re.findall(r'(\w+)=([^\s]+)',m[2]))
  rows.append(dict(arm=mark,run=run,frame=frame,EndScene_ms=fields.get('gxm_end','MISSING'),gxm_end_in=fields.get('gxm_end_in','MISSING'),sim_ms=fields.get('sim','MISSING'),render_ms=fields.get('render','MISSING'),present_ms=fields.get('present','MISSING')))
if a.json:print(json.dumps(rows,indent=2))
else:
 print('arm\trun\tframe\tEndScene_ms\tgxm_end_in\tsim_ms\trender_ms\tpresent_ms')
 for row in rows:print('\t'.join(map(str,row.values())))
 if not rows:raise SystemExit('No spike frames 2100..2130 found; use the bounded spike configs and wait for the flush before done.')
