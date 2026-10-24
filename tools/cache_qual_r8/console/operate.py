"""Explicit console-owner actions. Never run during Mac preparation."""
from pathlib import Path
import argparse, json, runpy, time
import pilot as p
from run_health import entry_for_label, health_text
def text():
    offset=json.loads((p.B/'LAUNCH.json').read_text())['log_offset']
    return p.LOG.read_bytes()[offset:].decode(errors='replace')
def wait_for(marker,seconds=20):
    deadline=time.monotonic()+seconds
    while True:
        t=text();reasons=health_text(t)
        if reasons:
            p.rec('STOP.json',dict(reasons=reasons));raise AssertionError(('STOP; release inputs and restore',reasons))
        if marker in t:return
        assert time.monotonic()<deadline,('missing native acknowledgement; never replay automatically',marker)
        time.sleep(.25)
def command(window,action,stride=0):
    assert action in ['start','stop']
    prior=json.loads((p.B/'COMMANDS.json').read_text()) if (p.B/'COMMANDS.json').exists() else []
    command=dict(id=len(prior)+1,action=action,label=window,check_stride=stride)
    local=p.B/f'command-{command["id"]}.json';local.write_text(json.dumps(command)+'\n')
    # Original presence/absence was preserved before first write. Never MKD.
    with p.Session(timeout=30) as s:s.execute('put',p.R+'cache_qual_command.json',str(local))
    read=p.snapshot(f'command-{command["id"]}-readback',[p.R+'cache_qual_command.json'])
    assert Path(read[0]['local']).read_bytes()==local.read_bytes()
    prior.append(command);p.rec('COMMANDS.json',prior)
    wait_for(f'[lg/cache-qual] {action} id={command["id"]} label={window}')
    return command
def window(label,name,resume,kind):
    entry_for_label(label);p.B=p.ROOT/label
    assert not (p.B/(name+'.json')).exists(),'Never repeat a measured window'
    stride=64 if kind=='check' else 0
    command(name,'start',stride)
    assert resume in ['circle','cross'];p.button(resume)
    sequence=[[64,128,2],[192,128,2]]*16 if kind=='move' else []
    started=time.monotonic_ns()//1000
    if kind=='move':
        for x,y,seconds in sequence:
            assert not health_text(text()),'live stop rule; do not continue'
            p.pulse('left-stick',x,y,seconds)
    else:
        deadline=time.monotonic()+5
        while time.monotonic()<deadline:
            assert not health_text(text()),'live stop rule; do not continue'
            time.sleep(.25)
    p.button('start')
    command(name,'stop',stride)
    v=dict(status='PASS',sequence=sequence,host_start_us=started,host_end_us=time.monotonic_ns()//1000,
           native_gameplay_duration_and_listener_motion='must pass audit; host waits are not proof',check_stride=stride)
    p.rec(name+'.json',v)
    if kind=='move':
        path=p.B/'MOTION.json';motion=json.loads(path.read_text()) if path.exists() else {};motion[name]=v;p.rec('MOTION.json',motion)
    print(p.shot(name+'-stopped'))
def main():
    a=argparse.ArgumentParser(description=__doc__);a.add_argument('label');a.add_argument('action',choices=['launch','button','shot','move','check','post','wait-exit','abort']);a.add_argument('args',nargs='*');v=a.parse_args()
    entry=entry_for_label(v.label);p.B=p.ROOT/v.label
    if v.action=='launch':
        assert json.loads((p.B/'STAGE.json').read_text())['status']=='PASS'
        identity=json.loads((p.B/'SELF-IDENTITY.json').read_text());expected=json.loads((p.ROOT.parent/'packages'/entry['package']/'VERIFICATION.json').read_text())
        assert identity['status']=='PASS' and identity['sha256']==expected['self_sha256']
        provenance=runpy.run_path(str(p.ROOT/'host-log-receiver.py'))['listener_provenance']()
        assert provenance['status']=='PASS';p.rec('HOST-LOGGER-PRELAUNCH.json',provenance)
        assert not (p.B/'LAUNCH.json').exists()
        p.rec('LAUNCH.json',dict(log_offset=p.LOG.stat().st_size,host_us=time.monotonic_ns()//1000))
        p.request('vita_launch',dict(title_id='ALICELG01'));time.sleep(20)
    elif v.action in ['move','check','post']:
        assert len(v.args)==2;return window(v.label,v.args[0],v.args[1],v.action)
    elif v.action=='button':
        assert 1<=len(v.args)<=2
        count=int(v.args[1]) if len(v.args)==2 else 1;assert 1<=count<=8
        for _ in range(count):p.button(v.args[0]);time.sleep(.15)
        time.sleep(1)
    elif v.action=='wait-exit':
        wait_for('Viewer closed cleanly.',45)
        deadline=time.monotonic()+10
        while text().rfind('[lg/cache-qual] exported ')<text().rfind('Viewer closed cleanly.'):
            assert time.monotonic()<deadline,'final export not completed';time.sleep(.25)
    elif v.action=='abort':
        p.cmd('release all');p.request('vita_quit',dict(title_id='ALICELG01'));time.sleep(2)
        p.rec('ABORT.json',dict(status='ABORTED_NOT_QUALIFIED'))
    print(p.shot(v.action+'-'+str(time.monotonic_ns())))
if __name__=='__main__':main()
