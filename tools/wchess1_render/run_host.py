"""One bounded shared-lock acquisition for dependent attribution/build/oracle work."""
import json,sys
import host_oracle as h
for tape,frame in [('a4-wchess1-01.lgtape',900),('rw-20261116-wchess1-01.lgtape',1700)]:
    h.acquire('wchess1 timing attribution: '+tape)
    try:
        h.run('census',tape,h.BINS/'lg-host-base',{},env_extra={
            'LOOKING_GLASS_RENDER_METRICS':'1',
            'LOOKING_GLASS_ACTOR_CENSUS':'1',
            'LOOKING_GLASS_DRAW_QUERY':'1',
            'LOOKING_GLASS_DRAW_QUERY_FRAME':str(frame),
            'LOOKING_GLASS_DRAW_QUERY_LAST':str(frame+2)})
    finally:h.release()
sys.argv=['build_host.py','arm']
import build_host
build_host.main()
rows=[]
for tape in h.TAPE_ROWS:
    reference=h.run('base',tape,h.BINS/'lg-host-base',{},decisions=True)
    for label in ('arm-off','alpha-once','skin-reuse','combined'):
        _,overrides,env=h.ARMS[label]
        arm=h.run(label,tape,h.BINS/'lg-host-arm',overrides,env_extra=env,decisions=True)
        rows.append(h.compare(reference,arm))
        (h.P/'host-proof.json').write_text(json.dumps(rows,indent=2)+'\n')
        print('ORACLE',tape,label,'PASS',flush=True)
