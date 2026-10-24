"""Reject invalid main admissions offline without modifying source evidence."""
from pathlib import Path
import argparse,contextlib,importlib.util,io,json,os,shutil,socket,subprocess,sys,tempfile
from unittest.mock import patch

def main():
 parser=argparse.ArgumentParser(description=__doc__)
 parser.add_argument('package',type=Path);parser.add_argument('--output',type=Path,required=True)
 args=parser.parse_args();source=args.package.resolve()
 if args.output.resolve().is_relative_to(source):parser.error('Output must stay outside the source package')
 spec=importlib.util.spec_from_file_location('e2e_fixture',Path(__file__).with_name('test_session_e2e.py'))
 e2e=importlib.util.module_from_spec(spec);spec.loader.exec_module(e2e)
 rows=[]
 def deny(*args,**kwargs):raise AssertionError('OFFLINE BOUNDARY: real transport/process denied')
 with tempfile.TemporaryDirectory(prefix='alice-r8-admission-negative-') as directory,contextlib.ExitStack() as stack:
  for obj,name in [(socket.socket,'connect'),(socket.socket,'connect_ex'),(socket.socket,'bind'),(socket,'create_connection'),(socket,'getaddrinfo'),(subprocess,'Popen'),(os,'system')]:stack.enter_context(patch.object(obj,name,deny))
  receipt=e2e.check_session_vacancy(source,True)
  work=Path(directory);package=work/'package';package.mkdir()
  shutil.copytree(source/'console',package/'console',ignore=shutil.ignore_patterns('__pycache__'))
  shutil.copyfile(source/'CONSOLE-PROTOCOL.json',package/'CONSOLE-PROTOCOL.json')
  (package/'packages/sah').mkdir(parents=True)
  for name in ['VERIFICATION.json','eboot.bin']:shutil.copyfile(source/'packages/sah'/name,package/'packages/sah'/name)
  smoke=e2e.copy_smoke_inputs(receipt,source/'session-s9-sah-load-smoke',package/'session-s9-sah-load-smoke')
  label=json.loads((package/'CONSOLE-PROTOCOL.json').read_text())['smoke_sequence'][0]['label']
  def reject(name,error_type,main_admission=True):
   output=work/(name+'.json')
   argv=['test_session_e2e.py',str(package),'--output',str(output)]+(['--main-admission'] if main_admission else [])
   with patch.object(sys,'argv',argv),contextlib.redirect_stdout(io.StringIO()) as captured:
    try:e2e.main()
    except error_type as error:message=str(error)
    else:raise AssertionError('Invalid admission accepted: '+name)
   result=json.loads(output.read_text());assert result['status']=='FAIL' and result['error_type']==error_type.__name__
   rows.append(dict(case=name,status='PASS',rejected=True,error_type=error_type.__name__,error=message,output=captured.getvalue()))
   print(f'PASS negative fixture: {name} rejected ({error_type.__name__})')
  smoke.rename(work/'held-smoke');reject('smoke-missing',FileNotFoundError);(work/'held-smoke').rename(smoke)
  for name,path in [('smoke-failed',smoke/label/'AUDIT.json'),('smoke-restore-failed',smoke/'RESTORE-FINAL.json')]:
   original=path.read_bytes();value=json.loads(original);value['status']='FAIL';e2e.write(path,value)
   try:reject(name,AssertionError)
   finally:path.write_bytes(original)
  for name,path in [('smoke-self-mismatch',smoke/label/'self-independent-readback.bin'),('smoke-save-mismatch',smoke/'restoration-readbacks/data/alice/private/saves/auto.json')]:
   with path.open('r+b') as file:original=file.read(1);file.seek(0);file.write(bytes([original[0]^1]))
   try:reject(name,AssertionError)
   finally:
    with path.open('r+b') as file:file.write(original)
  main_root=package/'session-s9-sah'
  main_root.mkdir();reject('main-path-present',FileExistsError);main_root.rmdir()
  main_root.write_text('occupied');reject('main-file-present',FileExistsError);main_root.unlink()
  main_root.symlink_to(work/'missing-target');reject('main-dangling-symlink-present',FileExistsError);main_root.unlink()
  reject('full-mode-rejects-existing-smoke',FileExistsError,False)
 e2e.write(args.output,dict(status='PASS',synthetic_offline_only=True,device_contact=False,driver_contact=False,cases=rows))
 print(f'PASS {len(rows)} negative admission fixtures; no device or driver contact')

if __name__=='__main__':main()
