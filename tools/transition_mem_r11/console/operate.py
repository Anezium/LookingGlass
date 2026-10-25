"""Explicit console-owner actions. Never run during Mac preparation."""
from pathlib import Path
import argparse, json, runpy, time
import pilot as p
from title_safety import record_closure, live_action
from run_health import entry_for_label, health_text
from transition_policy import check_record, LOAD_TIMEOUT_SECONDS
def text():
    offset=json.loads((p.B/'LAUNCH.json').read_text())['log_offset']
    return p.LOG.read_bytes()[offset:].decode(errors='replace')
def wait_transition(operation):
    target='pandemonium' if operation=='native-load-human-auto-pandemonium' else 'hedge3'
    expected_kind={'chapter-to-hedge3':'startup-entry','native-load-human-auto-pandemonium':'native-load','chapter-back-to-hedge3':'chapter-or-console','retry-hedge3':'death-retry'}[operation]
    consumed=json.loads((p.B/'TRANSITIONS.json').read_text()) if (p.B/'TRANSITIONS.json').exists() else []
    expected=['chapter-to-hedge3','native-load-human-auto-pandemonium','chapter-back-to-hedge3','retry-hedge3']
    assert len(consumed)<4 and operation==expected[len(consumed)], 'Never reorder or replay a transition'
    deadline=time.monotonic()+LOAD_TIMEOUT_SECONDS
    while True:
        raw=text();reasons=health_text(raw)
        if reasons:
            p.rec('STOP.json',dict(phase=operation,reasons=reasons));raise AssertionError(reasons)
        rows=[]
        for line in raw.splitlines():
            if line.startswith('[lg/transition-memory] '):
                row=json.loads(line.split('] ',1)[1])
                if row.get('event')=='end' and row.get('kind')!='quit':rows.append(row)
        if len(rows)>len(consumed):
            row=rows[len(consumed)]
            assert row['kind']==expected_kind and row['to']==target and row['success'], ('Unexpected transition',row)
            check_record(row)
            consumed.append(dict(operation=operation,record=row));p.rec('TRANSITIONS.json',consumed)
            if operation=='retry-hedge3':p.rec('RETRY-DECISION.json',dict(status='COMPLETED',source='inspected natural death and native Retry'))
            print(p.shot(operation+'-complete'));return
        if time.monotonic()>=deadline:
            reason='Missing native transition completion; stop, do not replay'
            p.rec('STOP.json',dict(phase=operation,reasons=[reason],loading_timeout_seconds=LOAD_TIMEOUT_SECONDS))
            raise AssertionError(reason)
        time.sleep(.25)
def menu_shot(name,label):
    image=p.shot(name)
    pending=json.loads((p.ROOT/'PENDING-IMAGE.json').read_text());pending.update(state='main-menu-candidate',run_label=label)
    p.rec(str(p.ROOT/'PENDING-IMAGE.json'),pending)
    return image
def main():
    a=argparse.ArgumentParser(description=__doc__);a.add_argument('label');a.add_argument('action',choices=['launch','button','shot','abort','prove-closed','wait-transition','resume','retry-unavailable']);a.add_argument('args',nargs='*');v=a.parse_args()
    entry=entry_for_label(v.label);p.B=p.ROOT/v.label
    if v.action in ['launch','button','abort']:p.owner();live_action()
    if v.action=='launch':
        assert json.loads((p.B/'STAGE.json').read_text())['status']=='PASS'
        identity=json.loads((p.B/'SELF-IDENTITY.json').read_text());expected=json.loads((p.ROOT.parent/'packages'/entry['package']/'VERIFICATION.json').read_text())
        assert identity['status']=='PASS' and identity['sha256']==expected['self_sha256']
        provenance=runpy.run_path(str(p.ROOT/'host-log-receiver.py'))['listener_provenance']()
        assert provenance['status']=='PASS';p.rec('HOST-LOGGER-PRELAUNCH.json',provenance)
        assert not (p.B/'LAUNCH.json').exists()
        p.rec('LAUNCH.json',dict(log_offset=p.LOG.stat().st_size,host_us=time.monotonic_ns()//1000))
        p.request('vita_launch',dict(title_id='ALICELG01'));time.sleep(20)
    elif v.action=='wait-transition':
        assert len(v.args)==1 and v.args[0] in ['chapter-to-hedge3','native-load-human-auto-pandemonium','chapter-back-to-hedge3','retry-hedge3']
        wait_transition(v.args[0]);return
    elif v.action=='retry-unavailable':
        assert not v.args and len(json.loads((p.B/'TRANSITIONS.json').read_text()))==3
        p.rec('RETRY-DECISION.json',dict(status='UNAVAILABLE_NO_VERIFIED_SAFE_DEATH_ROUTE',
            reason='Existing inspected inputs have no verified safe death route. No extra movement or console commands.'))
        return
    elif v.action=='resume':
        assert v.args==['circle']
        menu=json.loads((p.ROOT/'image-inspections.jsonl').read_text().splitlines()[-1])
        assert menu.get('state')=='main-menu' and menu.get('run_label')==v.label and menu.get('circle_back_resume_confirmed'), 'M3: Circle requires inspected modal Main menu'
        assert p.sha(Path(menu['path']).read_bytes())==menu['sha256']
        live_action();p.button('circle');print(p.shot('resumed'));return
    elif v.action=='button':
        assert 1<=len(v.args)<=2
        count=int(v.args[1]) if len(v.args)==2 else 1;assert 1<=count<=8
        assert v.args[0]!='start' or count==1
        for _ in range(count):p.button(v.args[0]);time.sleep(.15)
        time.sleep(1)
    elif v.action=='abort':
        p.cmd('release all');p.request('vita_quit',dict(title_id='ALICELG01'));time.sleep(2)
        p.rec('ABORT.json',dict(status='ABORTED_NOT_QUALIFIED'))
    elif v.action=='prove-closed':
        p.B=p.ROOT
        p.owner()
        closure=record_closure('prove-closed')
        image=p.shot('closure-home-'+str(time.monotonic_ns()))
        pending=json.loads((p.ROOT/'PENDING-IMAGE.json').read_text())
        pending.update(state='home-candidate',closure=closure)
        p.rec(str(p.ROOT/'PENDING-IMAGE.json'),pending)
        print(image);return
    if v.action=='button' and v.args==['start']:
        print(menu_shot('main-menu-'+str(time.monotonic_ns()),v.label));return
    print(p.shot(v.action+'-'+str(time.monotonic_ns())))
if __name__=='__main__':main()
