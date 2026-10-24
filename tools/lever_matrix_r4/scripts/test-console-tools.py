"""Offline fixtures only; synthetic and historical logs are never r4 console evidence."""
from pathlib import Path
import sys,json,csv,subprocess,tempfile,shutil,importlib.util,argparse,hashlib
def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output',type=Path)
    args=p.parse_args()
    O=Path(__file__).resolve().parents[1];sys.path.insert(0,str(O/'console'))
    from light_csv import read,summarize,check_observers
    results=[]
    def record(name,ok):
        assert ok,name
        results.append(dict(check=name,status='PASS'))
    with tempfile.TemporaryDirectory(prefix='observer-free-tool-tests-') as tmp:
        root=Path(tmp)
        phases=['world_opaque','world_alpha','actors','fx','hud','present']
        fields=['cpu_us','draws','pipelines','depth_states','bindings','uniform_uploads','uniform_bytes','buffer_updates','buffer_bytes','texture_bind_slots','visibility_tests','uniform_gl_calls','visibility_cpu_us','parameter_cpu_us','deferred_cpu_us','texture_gl_binds','state_gl_calls','program_gl_changes']
        header='frame,wall_us,frame_cpu_us,swap_us,skin_collect_wait_us,skin_gather_us,skin_fallbacks,skin_overflow,particle_producer_us,fx_merge_us,fx_upload_cpu_us,fx_upload_gl_us,fx_upload_bytes,fx_finish_prepare_us,fx_worker_wait_us,fx_finish_recycle_us'.split(',')+[p+'_'+f for p in phases for f in fields]+['gl_observers']
        csvpath=root/'synthetic.csv'
        with csvpath.open('w') as f:
            out=csv.writer(f);out.writerow(header)
            out.writerows([[i+1,60000]+['']*122+['direct'] for i in range(120)])
        summary=summarize(read(csvpath))
        record('125-column wall-only parser preserves every unavailable metric',summary['wall_us']['median']==60000 and all(summary[k] is None for k in header[2:-1]))
        protocol=O/'CONSOLE-PROTOCOL.json';seq=[x for x in json.loads(protocol.read_text())['sequence'] if x['block']=='s4-audio-hedge3']
        record('direct-mode protocol and empty-counter audit',check_observers(seq[0]['label'],summary,protocol)['status']=='PASS')
        damaged=dict(summary);damaged['world_opaque_draws']={'median':0}
        try: check_observers(seq[0]['label'],damaged,protocol)
        except AssertionError: record('reject fabricated zero observations',True)
        else: raise AssertionError('fabricated zero accepted')
        camera=dict(camera_eye=[1,2,3],camera_target=[4,5,6],camera_up=[0,0,1],feet=[1,2,0])
        def abba(values,expected,mutate=None):
            for entry,value in zip(seq,values):
                run=root/entry['label'];run.mkdir(exist_ok=True)
                audit=dict(status='PASS',fields_ms={'wall_us':dict(n=120,median=value,p95=value+1,sum=value*120)},gl_observers='direct',observer_checks=dict(status='PASS',expected_mode='direct'),runtime_config_matches_readback=True,complete_benchmark=True,clean_exit=True)
                if mutate:mutate(audit,entry)
                (run/'AUDIT.json').write_text(json.dumps(audit));(run/'STRICT-AUDIO.json').write_text('{"status":"PASS"}')
                (run/'ENTRY-STATE.json').write_text(json.dumps(dict(startup_camera_observations=[camera])))
            proc=subprocess.run([sys.executable,str(O/'console/compare-abba.py'),'--root',str(root),'--block','s4-audio-hedge3'],capture_output=True,text=True)
            if expected is None: record('reject mismatched observer evidence',proc.returncode!=0);return
            result=json.loads((root/'s4-audio-hedge3-ABBA-GATE.json').read_text())
            record(expected+' medians_ms='+str(values),result['status']==expected and proc.returncode==(0 if expected=='PASS_WALL_GATE' else 2))
        abba([60,61,61,60],'PASS_WALL_GATE')
        abba([60,61.001,61.001,60],'FAIL_WALL_GATE')
        abba([60,60.5,59.5,60],'PASS_WALL_GATE')
        abba([60,61.1,60.5,60],'INCONCLUSIVE_REPEAT_REQUIRED')
        abba([60,59.5,60.5,60],'PASS_WALL_GATE')
        abba([60,60,60,60],'PASS_WALL_GATE')
        abba([60,60,60,60],None,lambda audit,entry:audit.update(gl_observers='observed'))
        fixture_protocol=root/'fixture-protocol.json'
        fixture_protocol.write_text(json.dumps(dict(sequence=[dict(label=label,arm=arm) for label,arm in [('fixture-A','A'),('fixture-B','B'),('fixture-missing-B','B'),('fixture-starved-A','A')]])))
        audio='''bench: camera frozen map=hedge3
    [lg/audio-realtime] window_ms=1000 producer_blocks=20 producer_mix_p95_ms=9.6 pcm_starved_blocks=0 rejected_sinks=0 dropped_reports=0 queued_samples=100 producer_gap_max_ms=10 producer_sleep_overshoot_max_ms=1
    [lg/audio-music] decode_ms=1 underrun_frames=0 chunks=10
    [cpal/vita] late_callbacks=0 underrun_estimates=0 callbacks=20 callback_p95_us=100 callback_max_us=150
    bench: done 120 frames mode=fullbench profiler=light
    '''
        def strict(label,text,expected):
            run=root/label;run.mkdir(exist_ok=True);(run/'runtime-log.txt').write_text(text)
            subprocess.run([sys.executable,str(O/'console/strict-audio.py'),label,'--root',str(root),'--protocol',str(fixture_protocol)],capture_output=True,check=False)
            data=json.loads((run/'STRICT-AUDIO.json').read_text())
            record('strict audio '+label,data['status']==expected)
        strict('fixture-A',audio,'FAIL_OR_UNKNOWN')
        strict('fixture-B',audio,'FAIL_OR_UNKNOWN')
        strict('fixture-missing-B',audio.replace('pcm_starved_blocks=0',''),'FAIL_OR_UNKNOWN')
        strict('fixture-starved-A',audio.replace('pcm_starved_blocks=0','pcm_starved_blocks=1'),'FAIL_OR_UNKNOWN')
        if all((O/'packages'/side/'VERIFICATION.json').exists() for side in ('baseline', 'audio', 'world-static-merge')):
            for entry in json.loads(protocol.read_text())['sequence']:
                label=entry['label'];run=root/label;(run/'inputs').mkdir(exist_ok=True,parents=True)
                arm=entry['arm'];config=json.loads((O/'configs'/('PL-'+entry['config_arm']+'.json')).read_text())
                (run/'inputs/vita_renderer.json').write_text(json.dumps(config))
                sim=json.loads((O/'configs/vita-sim-all.json').read_text())
                (run/'inputs/vita_sim.json').write_text(json.dumps(sim))
                shutil.copy2(O/'configs/native-timing-disabled.json',run/'inputs/native-timing-config.json')
                shutil.copy2(O/'configs/profile_mode-fullbench.txt',run/'inputs/profile_mode')
                lines=['[lg/vita-renderer] config '+json.dumps(config),'[lg/vita-sim] config '+json.dumps(sim),
                       '[lg/startup] clocks mode=max rc=[0, 0, 0, 0] actual=[444, 222, 222, 166]',
                       '[lg/startup] map_load map='+entry['map'],'retained=true','[lg/audio-output] buffered=true',
                       'effective_priority=161 effective_affinity=0x40000',
                       'effective_priority=170 effective_affinity=0x40000',
                       'effective_priority=128 effective_affinity=0x20000']
                runtime='\n'.join(lines)+'\n'+('[lg/startup] restored scene reused map=pandemonium source=continue-or-bench\n' if entry['map']=='pandemonium' else '')+audio.replace('producer_mix_p95_ms=9.6','producer_mix_p95_ms=7.0').replace('bench: done','[lg/frame-total] measured=120 gl_hooks=0 phase_timers=0 clock=begin-to-begin\nbench: done')+'Viewer closed cleanly.\n'
                runtime=runtime.replace('map=hedge3','map='+entry['map'])
                if entry['map']=='pandemonium':
                    from map_identity import HUMAN_AUTO
                    (run/'SAVE-IDENTITY.json').write_text(json.dumps(dict(sha256=HUMAN_AUTO,bytes=181900,map='pandemonium$first',visit_key='pandemonium$first',canonical_map='pandemonium',fresh_session_start_readback=True,companions_restored_and_verified=True,independent_readbacks=2)))
                enabled=config['audio_offmain'];value=str(enabled).lower()
                runtime+=f'[lg/audio-offmain] requested={value} active={value} mode=static-traces-only\n'
                if enabled:runtime+='[lg/audio-offmain] submitted=12 reused=3 fallback=2\n'
                (run/'runtime-log.txt').write_text(runtime)
                doors=root/'synthetic-doors.csv';doors.write_text('frame_first,frame_last,frames,hatter_door_world_traces,hatter_door_own_traces,hatter_door_trace_pairs_deferred\n1,60,60,,,\n61,120,60,,,\n')
                (run/'COLLECT.json').write_text(json.dumps(dict(files=[dict(local=str(csvpath)),dict(local=str(doors))])))
                (run/'SELF-IDENTITY.json').write_text(json.dumps(dict(local_readback_path=str(O/'packages'/entry['package']/'eboot.bin'))))
                cmd=[sys.executable,str(O/'console/audit-run.py'),label,'--root',str(root),'--package',str(O)]
                proc=subprocess.run(cmd,capture_output=True,text=True)
                if proc.returncode:print(proc.stdout,proc.stderr)
                record('end-to-end synthetic wall audit '+arm,proc.returncode==0)
                for marker in ('Frame profile report failed', 'Viewer failed'):
                    (run/'runtime-log.txt').write_text(runtime+marker+'\n')
                    record('reject report failure '+arm+' '+marker,subprocess.run(cmd,capture_output=True).returncode!=0)
                (run/'runtime-log.txt').write_text(runtime.replace('[lg/frame-total]','[invalid-frame-total]'))
                record('reject missing frame-total receipt '+arm,subprocess.run(cmd,capture_output=True).returncode!=0)
    sealed=O/'proof/CONSOLE-TOOLS-TESTS.json'
    output=args.output or O/'proof/ROUND3-CONSOLE-TOOLS-TESTS.json'
    assert output.resolve()!=sealed.resolve(), 'Never rewrite the sealed proof through --output'
    payload=json.dumps(dict(status='PASS',synthetic_only=True,checks=results),indent=2)+'\n'
    output.write_text(payload)
    assert sealed.read_bytes()==payload.encode(), 'Fresh matrix results differ from sealed proof'
    artifact_seal=O/'ARTIFACT-SHA256.json'
    if artifact_seal.exists():
        expected=json.loads(artifact_seal.read_text())['proof/CONSOLE-TOOLS-TESTS.json']
        assert hashlib.sha256(sealed.read_bytes()).hexdigest()==expected
    integrity=subprocess.run([sys.executable,str(O/'scripts/test-log-integrity.py')],capture_output=True,text=True,timeout=30)
    assert integrity.returncode==0,integrity.stdout+integrity.stderr
    print(f'{len(results)} pure-host matrix tool checks passed.')
    print(integrity.stdout,end='')

if __name__=='__main__':main()
