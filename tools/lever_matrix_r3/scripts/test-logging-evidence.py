"""Verify production logging and retain strict rejection of the original failure."""
from pathlib import Path
import hashlib,json,os,subprocess,tempfile
O=Path(__file__).resolve().parents[1]
R=Path('/Users/anezium/vita-work/lg-lever-matrix-r3')
P=Path('/Users/anezium/vita-j0/resume-2026-10-23/alice-lever-matrix')
PY='/Users/anezium/vita-j0/venv/bin/python'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/Users/anezium/vitasdk/bin:/opt/homebrew/bin:'+os.environ['PATH'],PYTHONDONTWRITEBYTECODE='1')
test=subprocess.run([PY,str(R/'tools/test_atomic_logging.py')],capture_output=True,text=True,env=env,check=True)
(O/'logs/host-stdio-concurrency.log').write_text(test.stdout+test.stderr)
label='s3-fx-pandemonium-3-B';source=P/'session-s3-fx'/label/'runtime-log.txt'
assert '[lg/audio-realtime] window_ms=1000bench: done 120 frames mode=fullbench profiler=light' in source.read_text()
with tempfile.TemporaryDirectory(prefix='alice-original-audio-rejection-') as temp:
    root=Path(temp);run=root/label;run.mkdir();(run/'runtime-log.txt').write_bytes(source.read_bytes())
    proc=subprocess.run([PY,str(O/'console/strict-audio.py'),label,'--root',str(root),'--protocol',str(O/'CONSOLE-PROTOCOL.json')],env=env,capture_output=True,text=True)
    result=json.loads((run/'STRICT-AUDIO.json').read_text())
    assert proc.returncode==2 and result['status']=='FAIL_OR_UNKNOWN'
    assert 'UNKNOWN missing/corrupt/interleaved benchmark audio fields' in result['reasons']
    (O/'proof/ORIGINAL-B3-STRICT-REJECTION.json').write_text(json.dumps(dict(original_path=str(source),original_sha256=sha(source),returncode=proc.returncode,result=result),indent=2)+'\n')
assert sha(O/'console/strict-audio.py')==sha(P/'console/strict-audio.py')
assert '2 passed; 0 failed' in (O/'logs/host-logging.log').read_text()
proof=dict(status='PASS',console_contact=False,driver_contact=False,engine_file='src/logging.rs',
    upstream_writers={'audio':'src/audio/output.rs:568, eprintln!','completion':'src/viewer.rs:1184, println!'},
    sink_path='crate-wide Vita println/eprintln -> common Logger -> stdout/stderr -> libc write/_write_r -> existing debug transport or file descriptor; host receiver appends received bytes to vita.log',
    old_failure='separate stdout/stderr locks do not serialize formatting fragments into their shared downstream logging path',
    fix='format complete record+newline before shared mutex; acquire both stream locks in fixed order; write_all before unlock',
    direct_udp_socket_added=False,host_receiver_unchanged=sha(O/'console/host-log-receiver.py')==sha(P/'console/host-log-receiver.py'),
    tests=dict(fragmented_dual_sink_records=4000,production_pipe_records=4000,production_file_records=4000,
               writers_per_test=8,format_panic_recovery=True,all_records_exact_unique_newline_terminated=True),
    production_test_output=test.stdout.strip(),strict_audio_sha256=sha(O/'console/strict-audio.py'),
    strict_audio_unchanged=True,original_corrupt_B3_still_rejected=True,
    limitation='No device execution; libc/PrincessLog delivery and scheduling on Vita remain unmeasured. Test fanout uses byte buffers, pipe and file, never a socket.')
(O/'proof/LOGGING-PROOF.json').write_text(json.dumps(proof,indent=2)+'\n')
print('PASS: production pipe/file concurrency, fragmented dual sinks and unchanged strict rejection of original B3')
