from pathlib import Path
import sys
import hashlib,json,os,subprocess
O=Path(sys.argv[1]).resolve();env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/Users/anezium/vitasdk/bin:/opt/homebrew/bin:'+os.environ['PATH'])
rows={}
for name,cmd in [('host',['rustc','+stable','-Vv']),('native',['rustc','+nightly-2026-10-11','-Vv']),('cargo-vita',['cargo','vita','--version']),('uname',['uname','-a'])]:
 p=subprocess.run(cmd,env=env,capture_output=True,text=True);rows[name]={'argv':cmd,'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr}
for p in [Path('/Users/anezium/vitasdk/arm-vita-eabi/lib/libvitaGL.a'),Path('/Users/anezium/vitasdk/bin/vita-elf-create'),Path('/Users/anezium/vitasdk/bin/vita-make-fself')]:
 if p.exists():rows[str(p)]={'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
(O/'proof/TOOLCHAIN.json').write_text(json.dumps(rows,indent=2)+'\n')
