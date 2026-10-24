"""Offline amendment 1 regeneration and unchanged-artifact checks."""
from pathlib import Path
import contextlib,hashlib,io,json,os,runpy,socket,subprocess,sys,tempfile
from unittest.mock import patch
R=Path('/Users/anezium/vita-work/lg-phase-census-r5')
T=R/'tools/phase_census_r5'
O=Path(sys.argv[1]).resolve()
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def deny(*args,**kwargs):raise AssertionError('Offline check attempted transport or a subprocess')
with contextlib.ExitStack() as stack:
 for obj,name in [(socket.socket,'connect'),(socket.socket,'connect_ex'),(socket.socket,'bind'),(socket,'create_connection'),(socket,'getaddrinfo'),(subprocess,'Popen'),(os,'system')]:stack.enter_context(patch.object(obj,name,deny))
 files=list((O/'console').glob('*.py'))+list((O/'scripts').glob('*.py'))+list(T.rglob('*.py'))
 for path in files:compile(path.read_bytes(),str(path),'exec')
 print('PASS Python syntax for all package and clone helpers')
 with tempfile.TemporaryDirectory(prefix='alice-a1-regeneration-') as tmp:
  with patch.object(sys,'argv',['prepare.py',tmp]),contextlib.redirect_stdout(io.StringIO()):runpy.run_path(str(T/'prepare.py'),run_name='__main__')
  for path in (O/'console').glob('*.py'):assert path.read_bytes()==(Path(tmp)/'console'/path.name).read_bytes(),path
 print('PASS generated console helpers equal amended package helpers')
 for path in T.glob('*.py'):
  name=path.name
  assert (T/name).read_bytes()==(O/'scripts'/name).read_bytes(),name
 print('PASS clone and package amendment tools match')
 assert sha(O/'CONSOLE-BRIEF.md')=='48857506bc84cb02f322c1fa16603bcd94727cd5435dc96cab39a4fa1b2173c0'
 original=json.loads((O/'ARTIFACT-SHA256.json').read_text())
 for name,record in original.items():
  if name.startswith(('configs/','packages/')):assert sha(O/name)==record['sha256'],name
 for arm in ['baseline','census']:assert sha(O/'packages'/arm/'eboot.bin')=='50ac74a2ced3e4caf4ad239828df2dcbe1007f51be6b465471f00f5d3b918f69'
 print('PASS amended brief, original configs and common SELF identities')
 source=json.loads((O/'proof/SOURCE-IDENTITY.json').read_text())
 for name,digest in source['files'].items():assert sha(R/name)==digest,name
 print('PASS engine source matches preserved build identity')
 assert not list(O.rglob('__pycache__')) and not list(T.rglob('__pycache__'))
 print('PASS no helper bytecode caches')
print('All amendment 1 offline checks passed')
