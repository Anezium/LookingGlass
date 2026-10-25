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
 assert not os.path.lexists(source/'session-s10-stack') and not os.path.lexists(source/'session-s10-stack-load-smoke')
 with tempfile.TemporaryDirectory(prefix='alice-r8-admission-negative-') as directory,contextlib.ExitStack() as stack:
  for obj,name in [(socket.socket,'connect'),(socket.socket,'connect_ex'),(socket.socket,'bind'),(socket,'create_connection'),(socket,'getaddrinfo'),(subprocess,'Popen'),(os,'system')]:stack.enter_context(patch.object(obj,name,deny))
  work=Path(directory)
  smoke_result=e2e.exercise(source,work/'build-smoke',smoke=True)
  synthetic=Path(smoke_result['smoke_root'])
  verifier=e2e.load('negative_smoke_admission',source/'console/smoke_admission.py')
  try:verifier.inspect(source,synthetic)
  except AssertionError as error:assert 'synthetic' in str(error).lower(),error
  else:raise AssertionError('synthetic smoke at the exact smoke path was admitted')
  rows.append(dict(case='synthetic-at-exact-smoke-path',status='PASS',rejected=True,error_type='AssertionError'))
  print('PASS negative fixture: synthetic-at-exact-smoke-path rejected (AssertionError)')
  real=source/'session-s10-stack-load-smoke'
  assert not os.path.lexists(real)
  try:
   shutil.copytree(synthetic,real)
   try:verifier.inspect(source,real)
   except AssertionError as error:assert 'synthetic' in str(error).lower(),error
   else:raise AssertionError('synthetic smoke copied onto the package OUT path was admitted')
  finally:
   if os.path.lexists(real):shutil.rmtree(real)
  assert not os.path.lexists(real) and not os.path.lexists(source/'session-s10-stack')
  rows.append(dict(case='synthetic-copied-into-real-out',status='PASS',rejected=True,error_type='AssertionError'))
  print('PASS negative fixture: synthetic-copied-into-real-out rejected and removed')
  package=work/'package';package.mkdir()
  shutil.copytree(source/'console',package/'console',ignore=shutil.ignore_patterns('__pycache__'))
  shutil.copyfile(source/'CONSOLE-PROTOCOL.json',package/'CONSOLE-PROTOCOL.json')
  (package/'packages/stack').mkdir(parents=True)
  for name in ['VERIFICATION.json','eboot.bin']:shutil.copyfile(source/'packages/stack'/name,package/'packages/stack'/name)
  for name in ['PACKAGE-IDENTITY.json']:
   if (source/name).exists():shutil.copyfile(source/name,package/name)
  shaped=e2e.shape_real_smoke(synthetic,work/'shaped-smoke')
  assert verifier.inspect(package,shaped)['status']=='PASS'
  def reject(name,error_type,main_admission=True,fixture=shaped):
   output=work/(name+'.json')
   argv=['test_session_e2e.py',str(package),'--output',str(output)]
   if main_admission:argv+=['--main-admission','--smoke-fixture',str(fixture)]
   with patch.object(sys,'argv',argv),contextlib.redirect_stdout(io.StringIO()) as captured:
    try:e2e.main()
    except error_type as error:message=str(error)
    else:raise AssertionError('Invalid admission accepted: '+name)
   result=json.loads(output.read_text());assert result['status']=='FAIL' and result['error_type']==error_type.__name__
   rows.append(dict(case=name,status='PASS',rejected=True,error_type=error_type.__name__,error=message,output=captured.getvalue()))
   print(f'PASS negative fixture: {name} rejected ({error_type.__name__})')
  reject('smoke-missing',FileNotFoundError,fixture=work/'missing-smoke-fixture')
  protocol=json.loads((package/'CONSOLE-PROTOCOL.json').read_text())
  by_map={entry['map']:entry['label'] for entry in protocol['smoke_sequence']}
  for name,path in [('smoke-failed',shaped/by_map['hedge3']/'AUDIT.json'),('smoke-restore-failed',shaped/'RESTORE-FINAL.json')]:
   original=path.read_bytes();value=json.loads(original);value['status']='FAIL';e2e.write(path,value)
   try:reject(name,AssertionError)
   finally:path.write_bytes(original)
  for name,path in [('smoke-self-mismatch',shaped/by_map['hedge3']/'self-independent-readback.bin'),('smoke-save-mismatch',shaped/'restoration-readbacks/data/alice/private/saves/auto.json')]:
   with path.open('r+b') as file:original=file.read(1);file.seek(0);file.write(bytes([original[0]^1]))
   try:reject(name,AssertionError)
   finally:
    with path.open('r+b') as file:file.write(original)
  bare=e2e.shape_real_smoke(synthetic,work/'no-provenance')
  for path in [bare/'driver-ids.jsonl',bare/'DRIVER-READY.json',*bare.rglob('*.ftp.jsonl')]:
   if path.exists():path.unlink()
  try:verifier.inspect(package,bare)
  except AssertionError as error:assert 'driver' in str(error).lower() or 'FTP' in str(error) or 'DRIVER' in str(error),error
  else:raise AssertionError('real-looking smoke without driver provenance was admitted')
  rows.append(dict(case='real-looking-without-driver-provenance',status='PASS',rejected=True,error_type='AssertionError'))
  print('PASS negative fixture: real-looking-without-driver-provenance rejected')
  hedge=e2e.shape_real_smoke(synthetic,work/'hedge-missing-heap')
  audit_path=hedge/by_map['hedge3']/'AUDIT.json'
  audit=json.loads(audit_path.read_text());audit['heap_readout']['benchmark']['sampled_max_MiB']=None;audit['heap_readout']['benchmark']['count']=0;e2e.write(audit_path,audit)
  try:verifier.inspect(package,hedge)
  except AssertionError as error:assert 'Hedge3 smoke requires a benchmark heap sample under 165 MiB' in str(error),error
  else:raise AssertionError('Hedge3 smoke with a missing benchmark heap sample was admitted')
  rows.append(dict(case='hedge3-missing-benchmark-heap',status='PASS',rejected=True,error_type='AssertionError'))
  print('PASS negative fixture: hedge3-missing-benchmark-heap rejected')
  pand=e2e.shape_real_smoke(synthetic,work/'pandemonium-missing-heap')
  audit_path=pand/by_map['pandemonium']/'AUDIT.json'
  audit=json.loads(audit_path.read_text());audit['heap_readout']['benchmark']['sampled_max_MiB']=None;audit['heap_readout']['benchmark']['count']=0;e2e.write(audit_path,audit)
  admitted=verifier.inspect(package,pand)
  assert admitted['heap_benchmark_plateau_MiB']['pandemonium']=='unavailable'
  assert admitted['pandemonium_benchmark_heap_is_smoke_gate'] is False
  assert isinstance(admitted['heap_benchmark_plateau_MiB']['hedge3'],(int,float)) and admitted['heap_benchmark_plateau_MiB']['hedge3']<165
  rows.append(dict(case='pandemonium-missing-benchmark-heap-does-not-block',status='PASS',rejected=False,plateau=admitted['heap_benchmark_plateau_MiB']))
  print('PASS fixture: pandemonium missing benchmark heap is unavailable and is not a smoke gate')
  report=e2e.load('negative_make_report',source/'console/make-report.py')
  assert report.heap_status('pandemonium',[149,None,151,149])=='HEAP_NOT_CONFIRMED'
  assert report.heap_status('hedge3',[1,1,0,149])=='HEAP_NOT_CONFIRMED'
  assert report.heap_deltas([149,None,151,149]) is None
  rows.append(dict(case='make-report-missing-heap-sample',status='PASS',rejected=False,heap_status='HEAP_NOT_CONFIRMED'))
  print('PASS fixture: make-report missing heap sample is HEAP_NOT_CONFIRMED and does not raise')
  main_root=package/'session-s10-stack'
  main_root.mkdir();reject('main-path-present',FileExistsError);main_root.rmdir()
  main_root.write_text('occupied');reject('main-file-present',FileExistsError);main_root.unlink()
  main_root.symlink_to(work/'missing-target');reject('main-dangling-symlink-present',FileExistsError);main_root.unlink()
  occupied=package/'session-s10-stack-load-smoke';occupied.mkdir();reject('full-mode-rejects-existing-smoke',FileExistsError,False);occupied.rmdir()
 e2e.write(args.output,dict(status='PASS',synthetic_offline_only=True,device_contact=False,driver_contact=False,hardware_smoke=False,cases=rows))
 print(f'PASS {len(rows)} admission fixtures; no device or driver contact')

if __name__=='__main__':main()
