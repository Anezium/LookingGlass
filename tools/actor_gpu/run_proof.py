"""Final binary exact oracle; sequential GL runs, no timing claims or timing lock."""
import json,sys
import host_oracle as h
rows=[]
part=sys.argv[1] if len(sys.argv)>1 else "all"
tapes=sys.argv[2:] or list(h.TAPE_ROWS)
for tape in tapes:
 r=h.run('base',tape,h.BINS/'lg-host-base',{},decisions=('potears1' in tape))
 for label,overrides in [('env-off',{}),('env-discard-free',{'actor_discard_free_shader':True})]:
  a=h.run(label,tape,h.BINS/'lg-host-final',overrides,decisions=('potears1' in tape))
  rows.append(h.compare(r,a))
  (h.P/('host-proof-'+part+'.json')).write_text(json.dumps(rows,indent=2)+'\n')
  print('ORACLE PASS',tape,label,flush=True)
