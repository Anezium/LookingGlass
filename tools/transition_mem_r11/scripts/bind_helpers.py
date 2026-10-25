"""Verify a fresh session's helper bytes without touching the device or driver."""
from pathlib import Path
import argparse,hashlib,json

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,required=True);p.add_argument('--package',type=Path,default=Path(__file__).resolve().parents[1]);a=p.parse_args();root=a.root.resolve();package=a.package.resolve()
 assert root.parent==package and root.name=='session-s11-transition-mem'
 expected={f.name:hashlib.sha256(f.read_bytes()).hexdigest() for f in (package/'console').glob('*.py')}
 assert {f.name for f in root.iterdir()}==set(expected), 'Session must contain only freshly copied helpers'
 actual={f.name:hashlib.sha256(f.read_bytes()).hexdigest() for f in root.iterdir()};assert expected==actual
 result={'status':'PASS','root':str(root),'package':str(package),'helper_sha256':actual,'offline_only':True,'ownership_admitted':False}
 with (root/'HELPER-BINDING.json').open('x') as f:f.write(json.dumps(result,indent=2)+'\n')
 print('Fresh helper binding PASS; no ownership or transport action')
if __name__=='__main__':main()
