"""Assemble local console inputs. Does not import device or driver modules."""
from pathlib import Path
import hashlib,json,shutil,subprocess,sys
R=Path(__file__).resolve().parents[2];T=Path(__file__).resolve().parent;O=Path(sys.argv[1]).resolve()
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8')
for directory in ['console','scripts','configs']:
 (O/directory).mkdir(exist_ok=True)
 for source in (T/directory).glob('*'):
  if source.is_file():shutil.copyfile(source,O/directory/source.name)
(O/'configs/cache_qual_command.json').unlink(missing_ok=True)
shutil.copyfile(T/'CONSOLE-PROTOCOL.json',O/'CONSOLE-PROTOCOL.json')
rev=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()
write(O/'PACKAGE-IDENTITY.json',dict(package='alice-transition-mem-r11d',engine_source_revision=rev,branch=subprocess.check_output(['git','branch','--show-current'],cwd=R,text=True).strip(),branch_head=rev,console_executed=False))
for arm in ['baseline','stack']:
 files=[dict(local=f'configs/PL-{arm}.json',remote='ux0:/data/alice/private/vita_renderer.json',kind='small')]
 for local,remote in [('vita-sim-all.json','vita_sim.json'),('native-timing-disabled.json','native-timing-config.json'),('profile_mode-full.txt','profile_mode')]:
  files.append(dict(local='configs/'+local,remote='ux0:/data/alice/private/'+remote,kind='small'))
 write(O/f'stage-{arm}-full.json',dict(files=files))
write(O/'stage-baseline-self.json',dict(files=[dict(local='packages/baseline/eboot.bin',remote='ux0:/app/ALICELG01/eboot.bin',kind='self')]))
