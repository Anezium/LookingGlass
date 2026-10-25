"""Mac-local admission and boundary checks; no driver RPC or lifecycle action."""
import argparse, json, os, shlex, subprocess
from datetime import datetime, timezone, timedelta
from pathlib import Path

DRIVER=Path('/Users/anezium/vita-j0/resume-2026-10-21/timing-and-exit-followup/driver-v5/DRIVER-READY.json')

def timestamp(value):
    result=datetime.fromisoformat(value.replace('Z','+00:00'))
    assert result.tzinfo is not None, 'Timestamp needs UTC offset'
    return result

def driver_identity(ready):
    started=timestamp(ready['started']);expiry=timestamp(ready['fixed_expiry'])
    assert ready['lifetime_seconds']==43200, 'Driver must have a 12 hour lifetime'
    assert abs((expiry-started).total_seconds()-43200)<1, 'Expiry must equal started plus 12 hours'
    assert ready['auto_restart'] is False, 'Driver auto_restart must be false'
    if 'ready' in ready:assert ready['ready'] is True, 'Driver not ready'
    pid=int(ready['pid']);assert pid>0 and not isinstance(ready['pid'],bool), 'Invalid driver pid'
    try:os.kill(pid,0)
    except OSError as error:raise AssertionError('Driver pid is not alive') from error
    process=subprocess.run(['/bin/ps','-p',str(pid),'-o','command='],capture_output=True,text=True,check=False)
    assert process.returncode==0 and any(Path(token).name=='terminal-driver.py' for token in shlex.split(process.stdout)), 'Pid is not a live terminal-driver.py process'
    return dict(pid=pid,started=ready['started'],fixed_expiry=ready['fixed_expiry'])

def check(ready,start,now,launches=8,self_stages=1,pandemonium_launches=4,preservation_minutes=15,admitted=None):
    assert launches>0 and self_stages>=0 and 0<=pandemonium_launches<=launches
    assert preservation_minutes>=0
    identity=driver_identity(ready);expiry=timestamp(identity['fixed_expiry'])
    elapsed=(now-start).total_seconds()/60
    assert elapsed>=0
    remaining=preservation_minutes+9*launches+4.5*self_stages+1.5*pandemonium_launches+30
    left=(expiry-now).total_seconds()/60
    reasons=[]
    if admitted is not None and admitted!=identity:reasons.append('driver changed inside an admitted session')
    if now<timestamp(identity['started']):reasons.append('Driver start is in the future')
    if left<=remaining+30:reasons.append('Planned completion including restoration must be MORE THAN 30 minutes before expiry')
    if elapsed+remaining>150:reasons.append('Whole remainder plus restoration exceeds 150-minute session cap')
    return dict(status='STOP_RESTORE' if reasons else 'BUDGET_ADMITTED',reasons=reasons,
        fixed_expiry=identity['fixed_expiry'],driver_identity=identity,
        latest_admission_for_one_session=(expiry-timedelta(minutes=remaining+30)).isoformat(),
        remaining_minutes=remaining,driver_minutes_remaining=left,
        restoration_minutes=30,required_expiry_margin_minutes=30,elapsed_minutes=elapsed,
        readiness='Local pid and command line checked; sole ownership and current readiness require separate inspection; no driver call made')

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--driver-ready',type=Path,default=DRIVER)
    boundary=p.add_mutually_exclusive_group(required=True)
    boundary.add_argument('--admit',type=Path,help='Create a new driver identity receipt; never overwrite')
    boundary.add_argument('--admitted',type=Path,help='Existing admission receipt, required at every later boundary')
    p.add_argument('--session-start',required=True)
    p.add_argument('--now',help='Synthetic offline checks only')
    p.add_argument('--launches',type=int,default=8)
    p.add_argument('--self-stages',type=int,default=1)
    p.add_argument('--pandemonium-launches',type=int,default=4)
    p.add_argument('--preservation-minutes',type=float,default=15)
    a=p.parse_args()
    if a.admit:assert not a.admit.exists(), 'Admission receipt already exists; use --admitted at boundaries'
    result=check(json.loads(a.driver_ready.read_text()),timestamp(a.session_start),
        timestamp(a.now) if a.now else datetime.now(timezone.utc),a.launches,a.self_stages,
        a.pandemonium_launches,a.preservation_minutes,
        admitted=json.loads(a.admitted.read_text()) if a.admitted else None)
    if a.admit and not result['reasons']:
        with a.admit.open('x',encoding='utf-8') as out:out.write(json.dumps(result['driver_identity'],indent=2)+'\n')
        ready_copy=a.admit.with_name('DRIVER-READY.json')
        if not ready_copy.exists():ready_copy.write_text(a.driver_ready.read_text())
    print(json.dumps(result,indent=2))
    return 2 if result['reasons'] else 0

if __name__=='__main__':raise SystemExit(main())
