"""Local budget calculation only. Never invokes or restarts the driver."""
import argparse, json
from datetime import datetime, timezone
from pathlib import Path

DRIVER=Path('/Users/anezium/vita-j0/resume-2026-10-21/timing-and-exit-followup/driver-v5/DRIVER-READY.json')
EXPIRY='2026-10-24T07:10:31+02:00'

def timestamp(value):
    result=datetime.fromisoformat(value.replace('Z','+00:00'))
    assert result.tzinfo is not None, 'Timestamp needs UTC offset'
    return result

def check(ready,start,now,launches=8,self_stages=1,pandemonium_launches=4,preservation_minutes=15):
    assert launches>0 and self_stages>=0 and 0<=pandemonium_launches<=launches
    assert preservation_minutes>=0
    expiry=timestamp(EXPIRY)
    assert 0<=(timestamp(ready['fixed_expiry'])-expiry).total_seconds()<1, 'Unexpected driver expiry; never extend it'
    if 'ready' in ready:assert ready['ready'] is True, 'Driver not ready'
    elapsed=(now-start).total_seconds()/60
    assert elapsed>=0
    remaining=preservation_minutes+9*launches+4.5*self_stages+1.5*pandemonium_launches+30
    left=(expiry-now).total_seconds()/60
    reasons=[]
    if left<=remaining+30:reasons.append('Planned completion including restoration must be MORE THAN 30 minutes before expiry')
    if elapsed+remaining>150:reasons.append('Whole remainder plus restoration exceeds 150-minute session cap')
    return dict(status='STOP_RESTORE' if reasons else 'BUDGET_ADMITTED',reasons=reasons,
        fixed_expiry=EXPIRY,remaining_minutes=remaining,driver_minutes_remaining=left,
        restoration_minutes=30,required_expiry_margin_minutes=30,elapsed_minutes=elapsed,
        readiness='Actual driver readiness and sole ownership must be inspected separately; no driver call made')

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--driver-ready',type=Path,default=DRIVER)
    p.add_argument('--session-start',required=True)
    p.add_argument('--now',help='Synthetic offline checks only')
    p.add_argument('--launches',type=int,default=8)
    p.add_argument('--self-stages',type=int,default=1)
    p.add_argument('--pandemonium-launches',type=int,default=4)
    p.add_argument('--preservation-minutes',type=float,default=15)
    a=p.parse_args()
    result=check(json.loads(a.driver_ready.read_text()),timestamp(a.session_start),
        timestamp(a.now) if a.now else datetime.now(timezone.utc),a.launches,a.self_stages,
        a.pandemonium_launches,a.preservation_minutes)
    print(json.dumps(result,indent=2))
    return 2 if result['reasons'] else 0

if __name__=='__main__':raise SystemExit(main())
