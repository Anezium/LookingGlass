"""Offline subprocess end-to-end recording lifecycle against an inert fake kit."""
import hashlib,json,subprocess,sys,tempfile,unittest
from pathlib import Path
DRIVER=Path(__file__).with_name('record.py')
FAKE='''import sys,json
from pathlib import Path
p=Path(sys.argv[2]);args=sys.argv[3:];cmd=args[0]
with (p/'calls').open('a') as f:f.write(' '.join(args)+'\\n')
if cmd=='log': print('RESULT: PASS')
if cmd=='recover': print('AWAITING ROOT OBSERVATION: fixture.png')
if cmd=='tape':
 (p/'tapes').mkdir(exist_ok=True);(p/'tapes'/args[2]).write_text('fixture tape')
if cmd=='restore':
 assert args==['restore','--keep-saves'];print('RESTORE VERIFIED')
'''
class Record(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name)
  (self.root/'logs').mkdir();(self.root/'logs/arms.log').write_text('old receiver rows\n')
  (self.root/'state.json').write_text(json.dumps(dict(game='closed',pending=None)))
  self.kit=self.root/'fake-kit.py';self.kit.write_text(FAKE)
  self.pin=hashlib.sha256(self.kit.read_bytes()).hexdigest()
  self.binary=self.root/'test.self';self.binary.write_bytes(b'fixture self')
  self.cfg=self.root/'record.json';self.cfg.write_text(json.dumps(dict(replay_record='fp-hatter2-01.lgtape',replay_play='',replay_start='chapter:hatter2@normal',replay_fight_pilot=True)))
 def tearDown(self): self.tmp.cleanup()
 def run_step(self,action,ok=True,*extra):
  r=subprocess.run([sys.executable,str(DRIVER),action,'--session',str(self.root),'--kit',str(self.kit),'--kit-sha256',self.pin,*extra],capture_output=True,text=True)
  self.assertEqual(r.returncode==0,ok,r.stdout+r.stderr)
 def start(self):self.run_step('start',True,'--self',str(self.binary),'--config',str(self.cfg))
 def clean(self):
  with (self.root/'logs/arms.log').open('a') as f:f.write('[lg/replay] recorded fp-hatter2-01.lgtape steps=1200 reason=menu checks=40 events=1 seconds=60.0\n')
 def test_lifecycle(self):
  self.start();self.clean();self.run_step('recover');self.run_step('get');self.run_step('restore')
  calls=(self.root/'calls').read_text();self.assertIn('restore --keep-saves',calls);self.assertNotIn('restore\n',calls)
  self.assertTrue((self.root/'fp-hatter2-01.lgtape.receipt.json').exists())
 def test_missing_clean_line_blocks_recover(self):self.start();self.run_step('recover',False)
 def test_no_second_take(self):self.start();self.start_fails()
 def start_fails(self):self.run_step('start',False,'--self',str(self.binary),'--config',str(self.cfg))
 def test_missing_observation_blocks_get(self):
  self.start();self.clean();(self.root/'state.json').write_text(json.dumps(dict(game='running',pending=None)));self.run_step('get',False)
 def test_pinned_kit(self):
  self.start();self.kit.write_text(FAKE+'\n');self.clean();self.run_step('recover',False)
if __name__=='__main__':unittest.main()
