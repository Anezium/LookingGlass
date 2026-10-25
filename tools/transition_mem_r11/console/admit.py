"""Future owner's local en0 check and explicit ownership receipt; no driver RPC."""
from pathlib import Path
import argparse, json, subprocess
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--sole-console-owner',action='store_true',required=True);a=p.parse_args()
    root=Path(__file__).resolve().parent
    assert root.name=='session-s11-transition-mem' and (root/'HELPER-BINDING.json').exists()
    assert (root/'DRIVER-ADMISSION.json').exists(),'First perform local fixed-driver budget admission'
    assert not (root/'owner-returned.json').exists()
    # Route query is deliberately deferred to the future admitted console owner.
    r=subprocess.run(['/sbin/route','-n','get','192.168.1.23'],capture_output=True,text=True,check=True)
    assert 'interface: en0' in r.stdout,'en0 required; do not touch console ports'
    for name,data in [('PREFLIGHT.json',dict(interface='en0',route=r.stdout)),('OWNERSHIP.json',dict(sole_console_owner=True,explicit_owner_assertion=True))]:
        with (root/name).open('x') as out:out.write(json.dumps(data,indent=2)+'\n')
    print('en0 and explicit ownership recorded; no console or driver RPC')
if __name__=='__main__':main()
