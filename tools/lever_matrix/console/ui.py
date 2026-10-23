import argparse

def main():
    parser=argparse.ArgumentParser(description='LABEL ACTION [ARG ...]; actions: launch, button, select, chapters, load-auto, begin, resume, close, shot')
    parser.add_argument('arguments',nargs='*')
    parser.parse_args()
    import pilot as p,sys,time
    label,action=sys.argv[1:3]
    from run_health import arm_for_label, entry_for_label
    arm=None if label=='root' else arm_for_label(label)
    assert label!='root' or action in ['button','close','shot'], 'Bench actions require an exact protocol arm'
    p.B=p.ROOT if label=='root' else p.ROOT/label;p.B.mkdir(exist_ok=True)
    if action=='launch':
     assert p.json.loads((p.B/'STAGE.json').read_text())['status']=='PASS','Launch requires completed per-arm staging/readbacks'
     import subprocess
     health=subprocess.run(['/usr/sbin/lsof','-nP','-iTCP:9999','-sTCP:LISTEN'],capture_output=True,text=True)
     p.rec('HOST-LOGGER-PRELAUNCH.json',dict(time=p.now(),returncode=health.returncode,listener=health.stdout))
     assert health.returncode==0 and 'LISTEN' in health.stdout,'Host PrincessLog receiver absent; do not launch'
     logfile=p.Path('/Users/anezium/vita-j0/work/vita.log')
     p.rec('LAUNCH.json',dict(time=p.now(),host_us=time.monotonic_ns()//1000,log_offset=logfile.stat().st_size));print(p.request('vita_launch',dict(title_id='ALICELG01')),flush=True);time.sleep(20)
    elif action=='button':p.button(sys.argv[3]);time.sleep(float(sys.argv[4]) if len(sys.argv)>4 else 1)
    elif action in ['select','chapters']:
     if action=='chapters':p.button('select')
     count=int(sys.argv[3])
     if action=='chapters':
      for offset in range(0,count,2):
       p.request('vita_press',dict(sequence='; '.join(['press down; wait 120ms; release all; wait 100ms']*min(2,count-offset))))
     else:
      for _ in range(count):p.button('down')
    elif action=='load-auto':
     assert entry_for_label(label)['entry']=='Load Save'
     # The native menu's linear order was inspected and checked against menu.rs:
     # Main LoadSave, slot0..5 (Auto), Load6. Stop on Load without activating it.
     p.button('down');p.button('cross');time.sleep(.2)
     for _ in range(5):p.button('down')
     p.button('cross');p.button('down');time.sleep(.5)
    elif action=='begin':
     assert entry_for_label(label)['entry']=='Chapters', 'Use inspected native Load Save activation for Pandemonium'
     p.cmd('release all');p.request('vita_press',dict(sequence='press front-touch 0 630 1000; wait 100ms; release all'))
     deadline=time.monotonic()+(55 if entry_for_label(label)['map']=='hedge3' else 40)
     from run_health import health_text
     offset=p.json.loads((p.B/'LAUNCH.json').read_text())['log_offset']
     while time.monotonic()<deadline:
      text=p.Path('/Users/anezium/vita-j0/work/vita.log').read_bytes()[offset:].decode(errors='replace')
      reasons=health_text(text,A=arm=='A')
      if reasons:
       p.rec('STOP.json',dict(time=p.now(),phase='entry-before-benchmark',reasons=reasons))
       print('STOP RULE: '+p.json.dumps(reasons),flush=True)
       print(p.request('vita_quit',dict(title_id='ALICELG01')),flush=True)
       time.sleep(2)
       break
      time.sleep(1)
    elif action=='resume':
     quiet_seconds=45
     resume_button=sys.argv[3] if len(sys.argv)>3 else 'circle'
     assert resume_button in ['circle','cross']
     p.cmd('release all');quiet=dict(time=p.now(),host_us=time.monotonic_ns()//1000,post_resume_observation_delay_seconds=quiet_seconds,resume_button=resume_button)
     p.button(resume_button);quiet.update(resume_input_completed_at=p.now(),resume_input_completed_host_us=time.monotonic_ns()//1000);p.rec('QUIET-START.json',quiet)
     print(f'RESUMED, PASSIVE LOG OBSERVATION ONLY',flush=True)
     # Passive host-log observation only. Never take a screenshot of a benchmark
     # that has not self-terminated, even if the expected time has elapsed.
     offset=p.json.loads((p.B/'LAUNCH.json').read_text())['log_offset'];deadline=time.monotonic()+75
     while True:
      text=p.Path('/Users/anezium/vita-j0/work/vita.log').read_bytes()[offset:].decode(errors='replace')
      from run_health import health_text
      reasons=health_text(text,A=arm=='A')
      if reasons:
       p.rec('STOP.json',dict(time=p.now(),reasons=reasons))
       print('STOP RULE: '+p.json.dumps(reasons),flush=True)
       print(p.request('vita_quit',dict(title_id='ALICELG01')),flush=True)
       time.sleep(2)
       quiet.update(stop_seen_at=p.now(),reason='audio/heap stop rule',normal_termination=False);p.rec('QUIET-START.json',quiet)
       break
      if 'bench: done 120 frames mode=fullbench' in text and 'Viewer closed cleanly.' in text:break
      assert time.monotonic()<deadline,'NO NORMAL FULLBENCH TERMINATION: no screenshot taken; diagnose/abort and restore'
      time.sleep(1)
     if not reasons:
      quiet.update(normal_termination_seen_at=p.now());p.rec('QUIET-START.json',quiet)
    elif action=='close':print(p.request('vita_quit',dict(title_id='ALICELG01')),flush=True);time.sleep(2)
    elif action=='shot':pass
    else:raise ValueError(action)
    capture_label=sys.argv[3] if action=='shot' and len(sys.argv)>3 else action+('-'+sys.argv[3] if action in ['button','select','chapters'] else '')
    if action=='button' and len(sys.argv)>5:capture_label=sys.argv[5]
    print(p.shot(capture_label),flush=True)

if __name__=='__main__':
    main()
