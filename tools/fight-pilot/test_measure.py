"""Fake-kit CLI end-to-end test of native fight tape to gated RC14 p95 output."""
import hashlib,json,os,subprocess,sys,tempfile,unittest
from pathlib import Path
SOURCE=Path(__file__).resolve().parent
FAKE='''import contextlib,hashlib,json,sys
from pathlib import Path
class Stop(Exception): pass
class Sess:
 def __init__(self,p): self.root=p
@contextlib.contextmanager
def session_lock(*a): yield
def mutation_guard(*a): pass
def receiver_alive(*a): return True
def assert_transaction(s,t): assert t=='fixture'
def parse_log(p,connections=False): return [(None,None,'vita',l,1) for l in p.read_text().splitlines()]
def launch_matches(rows,launch): return launch=='fixture'
def launch_lines(rows): return rows
if __name__=='__main__':
 root=Path(sys.argv[2]);args=sys.argv[3:];state=json.loads((root/'state.json').read_text());cmd=args[0]
 if cmd in ('install','config'):
  cfg=Path(args[-1]);state['config']={'sha256':hashlib.sha256(cfg.read_bytes()).hexdigest()};state['fixture_config']=json.loads(cfg.read_text())
  if cmd=='install':state['installed']={'eboot':hashlib.sha256(Path(args[1]).read_bytes()).hexdigest()}
 elif cmd=='launch':
  c=state['fixture_config'];t=json.loads((root/'tape-index.json').read_text())[c['replay_play']]
  lines=[f'[lg/spike] frame={i} ms=40.00 sim=15.00 render=20.00' for i in range(c['spike_log_first_frame'],c['spike_log_last_frame']+1)]
  lines+=['[lg/spike] flushed at=replay-end lines=2 dropped=0',f'[lg/replay] done {c["replay_play"]} steps={t["steps"]}/{t["steps"]} reason=tape-end checks={t["checks"]} match={t["checks"]} diff=0 first_diff=- event_mismatches=0 seconds=1.0 result=PASS']
  (root/'logs/arms.log').write_text('\\n'.join(lines)+'\\n')
 elif cmd=='wait-exit':
  image=root/'observation.png';image.write_bytes(b'fake PNG');digest=hashlib.sha256(image.read_bytes()).hexdigest();state['observation']={'path':str(image),'sha256':digest,'quit_outcome':'not-running','transaction':'fixture'}
  print('AWAITING ROOT OBSERVATION: '+str(image)+'\\nSHA-256 '+digest+'\\nquit reply: fixture (not-running).')
 elif cmd=='log':print('RESULT: PASS')
 (root/'state.json').write_text(json.dumps(state))
'''
class Measure(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name).resolve()
  for n in ['tapes','logs']: (self.root/n).mkdir()
  self.kit=self.root/'fakekit.py';self.kit.write_text(FAKE)
  self.binary=self.root/'RC14.self';self.binary.write_bytes(b'fake SELF')
  tape=self.root/'tapes/fp-fixture.lgtape';tape.write_text('LGTAPE 1\nH '+json.dumps(dict(version=1,platform='vita'))+'\nE 0 map:hatter2\nC 0 0123456789abcdef - hatter2 0,0,0 100 1\nF 3d088889 pad=80808080\nF 3d088889\nEND '+json.dumps(dict(steps=2,checks=1,reason='menu'))+'\n')
  (self.root/'session.jsonl').write_text(json.dumps(dict(command='tape get',local=str(tape),sha256=hashlib.sha256(tape.read_bytes()).hexdigest()))+'\n')
  self.windows=self.root/'windows.json';self.windows.write_text(json.dumps(dict(windows=[dict(tape=tape.name,window=[1,2])])))
  (self.root/'state.json').write_text(json.dumps(dict(game='closed',pending=None,launch_id='fixture')))
  self.env=dict(os.environ,PLAY_RECORD_KIT=str(self.kit),FIGHT_PILOT_SESSION=str(self.root))
  self.call('prepare-measure.py','--session',str(self.root),'--windows',str(self.windows),'--rc14-self',str(self.binary),'--kit',str(self.kit),'--kit-sha256',hashlib.sha256(self.kit.read_bytes()).hexdigest(),'--rc14-sha256',hashlib.sha256(self.binary.read_bytes()).hexdigest())
 def tearDown(self):self.tmp.cleanup()
 def call(self,*args,ok=True):
  r=subprocess.run([sys.executable,str(SOURCE/args[0]),*args[1:]],env=self.env,capture_output=True,text=True)
  self.assertEqual(r.returncode==0,ok,r.stdout+r.stderr)
 def test_native_tape_to_p95(self):
  self.call('measure.py','--step');row=json.loads((self.root/'report.json').read_text())[0]
  self.assertEqual((row['variant'],row['map'],row['ms']['p95']),('rc14','hatter2',40.))
  self.assertEqual(row['scope'],'SELECTED_WINDOWS_ONLY');self.call('measure.py','--step',ok=False)
 def test_changed_kit_stops(self):self.kit.write_text(FAKE+'\n');self.call('measure.py','--step',ok=False)
 def test_changed_tape_stops(self):
  (self.root/'tapes/fp-fixture.lgtape').write_text('changed');self.call('measure.py','--step',ok=False)
 def test_dropped_lines_stop(self):
  self.kit.write_text(FAKE.replace('dropped=0','dropped=1'));manifest=json.loads((self.root/'READY.json').read_text());manifest['kit_sha256']=hashlib.sha256(self.kit.read_bytes()).hexdigest();(self.root/'READY.json').write_text(json.dumps(manifest));self.call('measure.py','--step',ok=False)
 def test_mismatch_stops(self):
  self.kit.write_text(FAKE.replace('diff=0','diff=1'));manifest=json.loads((self.root/'READY.json').read_text());manifest['kit_sha256']=hashlib.sha256(self.kit.read_bytes()).hexdigest();(self.root/'READY.json').write_text(json.dumps(manifest));self.call('measure.py','--step',ok=False)
if __name__=='__main__':unittest.main()
