from pathlib import Path
import os,sys,json,time,datetime,hashlib,subprocess,shutil,re,signal
ROOT=Path(__file__).resolve().parent
B=Path(os.environ.get('GATE_RUN_DIR',str(ROOT)))
R='ux0:/data/alice/private/';A='ux0:/app/ALICELG01/'
sys.path.insert(0,'/Users/anezium/vita-j0/bin')
from vita_ftp import Session
os.environ['VITA_FTP_LOG']=str(B/'ftp.jsonl')
def now():return datetime.datetime.now().astimezone().isoformat()
def sha(x):return hashlib.sha256(x).hexdigest()
def rec(n,v):
 p=B/n;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(v,indent=2)+'\n')
def event(n,v):
 with (B/n).open('a') as f:f.write(json.dumps(dict(time=now(),**v))+'\n')
def owner():
 assert not (ROOT/'PENDING-IMAGE.json').exists(),'Inspect and acknowledge returned image before another device action'
 assert not (ROOT/'owner-returned.json').exists(),'ROOT OWNERSHIP RETURNED'
 assert not (B/'owner-returned.json').exists(),'OWNERSHIP RETURNED: NO PORTS'
def request(name,args={}):
 assert name in {"vita_shot","vita_cmd","vita_press","vita_launch","vita_quit"}, ("Driver-disallowed tool",name)
 owner()
 wall_started=time.time()
 pre=time.monotonic_ns()//1000
 out=subprocess.check_output(['/Users/anezium/vita-j0/venv/bin/python','/Users/anezium/vita-j0/resume-2026-10-21/timing-and-exit-followup/driver-v5/driver-call.py',name,json.dumps(args)],text=True)
 post=time.monotonic_ns()//1000
 try:v=json.loads(out)
 except json.JSONDecodeError:
  # The driver publishes a response path before its write completes. Recover
  # that exact completed request; never replay an input after an empty read.
  driver=Path('/Users/anezium/vita-j0/resume-2026-10-21/timing-and-exit-followup/driver-v5')
  candidates=[]
  for f in driver.joinpath('driver-requests').glob('*.json'):
   if f.stat().st_mtime<wall_started-.1:continue
   req=json.loads(f.read_text())
   if req['name']==name and req.get('arguments',{})==args:candidates.append(f)
  assert len(candidates)==1,('Ambiguous response recovery',candidates)
  response=driver/'driver-responses'/candidates[0].name
  deadline=time.monotonic()+2
  while True:
   try:v=json.loads(response.read_text());break
   except (FileNotFoundError,json.JSONDecodeError):
    if time.monotonic()>deadline:raise
    time.sleep(.05)
  event('response-recovery.jsonl',dict(request=str(candidates[0]),response=str(response),no_replay=True))
 rec('driver-responses/'+datetime.datetime.now().strftime('%H%M%S-%f')+'.json',v)
 assert not v.get('exception') and not v['result'].get('isError'),v
 event('commands.jsonl',dict(name=name,args=args,started=v.get('started'),ended=v.get('ended'),host_pre_us=pre,host_post_us=post))
 result=v['result']['structuredContent']['result']
 assert not (isinstance(result,str) and result.startswith('Error:')),result
 return result
def cmd(c):
 assert c in ['version','help','release all','screen status','nosleep status','appstate','appstate PTVITA004','appstate ALICELG01']
 return request('vita_cmd',dict(raw=c))
def pulse(stick,x,y,seconds):
 assert 0<seconds<=8 and stick in ['left-stick','right-stick']
 return request('vita_press',dict(sequence=f'press {stick} {x} {y}; wait {round(seconds*1000)}ms; release all'))
def button(name):
 assert name in ['start','cross','circle','up','down','left','right','ps','select']
 hold=2500 if name=='start' else 120
 return request('vita_press',dict(sequence=f'press {name}; wait {hold}ms; release all'))
def shot(label):
 src=Path(request('vita_shot',dict(name='integrated-'+label)));dst=B/'captures'/(label+'.png');dst.parent.mkdir(exist_ok=True);shutil.copy2(src,dst)
 event('capture-anchors.jsonl',dict(label=label,path=str(dst),sha256=sha(dst.read_bytes())))
 rec(str(ROOT/'PENDING-IMAGE.json'),dict(path=str(dst),label=label))
 return str(dst)
def listing(s,path):
 owner();raw=s.execute('list',path);event('inventories.jsonl',dict(root=path,raw=raw));rows={}
 for l in raw.splitlines():
  a=l.split(None,8)
  if len(a)==9 and a[8] not in ['.','..']:rows[a[8]]=dict(bytes=int(a[4]),directory=l.startswith('d'),raw=l)
 return rows
def get(s,remote,local,stable=True,expected=None):
 owner()
 pending=ROOT/'pending-USB-collection.jsonl'
 if pending.exists():assert remote not in {json.loads(line)['path'] for line in pending.read_text().splitlines()},('NO FTP RETRY: pending USB collection',remote)
 local=Path(local);local.parent.mkdir(parents=True,exist_ok=True);s.ftp.voidcmd('TYPE I');n=expected if expected is not None else s.execute('size',remote)
 fullbench=remote.endswith('-fullbench-120.json') and remote.startswith(R+'frame_profile/')
 assert n<=4_000_000,('TRANSFER SIZE STOP',remote,n)
 previous_alarm=None
 if fullbench:
  def timeout_alarm(signum,frame):raise TimeoutError('Fullbench download exceeded 120-second wall deadline')
  previous_alarm=signal.signal(signal.SIGALRM,timeout_alarm);signal.setitimer(signal.ITIMER_REAL,120)
 try:s.execute('get',remote,str(local))
 except (TimeoutError, __import__('socket').timeout):
  if fullbench:event(str(ROOT/'pending-USB-collection.jsonl'),dict(path=remote,bytes=n,status='pending USB collection',reason='Download timed out; no retry; retain app-created export on Vita'))
  raise
 finally:
  if fullbench:signal.setitimer(signal.ITIMER_REAL,0);signal.signal(signal.SIGALRM,previous_alarm)
 x=local.read_bytes()
 if stable:assert len(x)==n,(remote,n,len(x))
 return x
def snapshot(tag,paths):
 m=[]
 with Session(timeout=120 if any(p.endswith('-fullbench-120.json') for p in paths) else 120) as s:
  inv={d:listing(s,d) for d in sorted({p.rsplit('/',1)[0]+'/' for p in paths})};rec(tag+'/inventories.json',inv)
  for p in paths:
   d,n=p.rsplit('/',1);v=dict(path=p,present=n in inv[d+'/'])
   if v['present']:
    assert not inv[d+'/'][n]['directory'];dst=B/tag/p.removeprefix('ux0:/');x=get(s,p,dst,expected=inv[d+'/'][n]['bytes']);v.update(local=str(dst),bytes=len(x),sha256=sha(x))
   m.append(v);rec(tag+'/manifest.partial.json',m)
 rec(tag+'/manifest.json',m);return m
def log(tag):
 dst=B/tag/'log.txt'
 dst.parent.mkdir(parents=True,exist_ok=True)
 with Session(timeout=10) as s:s.execute('get',R+'log.txt',str(dst))
 x=dst.read_bytes()
 event('log-reads.jsonl',dict(tag=tag,bytes=len(x),sha256=sha(x)));return x.decode(errors='replace')

if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description='Import-only support library; --help performs no device action.')
    parser.parse_args()
