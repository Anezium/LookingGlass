"""Timed host family scopes in the requested windows; separate instrumented binary."""
import gzip,json,os,shutil,subprocess,statistics
import host_oracle as h
TAPES={'a4-wchess1-01.lgtape':[(700,1180)],'rw-20261116-wchess1-01.lgtape':[(796,1276),(1486,1966)]}
def main():
    summaries=[]
    for tape,windows in TAPES.items():
        out=h.P/'host'/(tape+'-family-cpu')
        out.mkdir(exist_ok=False);private=out/'private';private.mkdir()
        config=json.loads((h.SOURCE/'configs/PL-release.json').read_text())
        config.update(replay_play=tape,phase_census=False,profiler='full',spike_log_ms=0,log_frame_time_every=0)
        (private/'vita_renderer.json').write_text(json.dumps(config))
        shutil.copy2(h.SOURCE/'configs/vita-sim/vita-sim-release.json',private/'vita_sim.json')
        source=h.P.parent/('release-week/play-record/tapes' if tape.startswith('rw-') else 'a5-heavy/tapes')/tape
        (private/tape).symlink_to(source)
        env={k:os.environ[k] for k in ['HOME','USER','TMPDIR'] if k in os.environ}
        env.update(PATH='/usr/bin:/bin',LANG='C',LOOKING_GLASS_REPLAY_CROSS='1',LOOKING_GLASS_VITA_CONTROLS='1',LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_PROBE_KEEP_WINDOW='1',LOOKING_GLASS_RENDER_METRICS='1',LOOKING_GLASS_FRAME_PROFILE='1')
        rows=[]
        h.acquire('wchess1 host family timing: '+tape)
        try:
            with (out/'stdout.log').open('w') as log,(out/'stderr.log').open('w') as err:
                result=subprocess.run([str(h.BINS/'lg-host-attribution'),'--data',str(h.DATA),'--save-dir',str(private/'saves'),'--frames',str(max(b for _,b in windows)+2)],cwd=out,env=env,stdout=log,stderr=err,timeout=600)
            assert result.returncode==0,(tape,result.returncode)
        finally:h.release()
        with (out/'stdout.log').open() as f:
            for line in f:
                if line.startswith('SIM_MAPS_FRAME '):
                    r=json.loads(line.split(' ',1)[1]);rows.append({k:r[k] for k in ['frame','active','render','spans_ms']})
        for first,last in windows:
            chosen=[r for r in rows if first<=r['frame']+1<=last]
            assert len(chosen)==481 and all(isinstance(r['render'],dict) for r in chosen)
            phases={name:{key:statistics.mean(r['render'][name][key] for r in chosen) for key in chosen[0]['render'][name]} for name in chosen[0]['render']}
            spans={name:statistics.mean(r['spans_ms'].get(name,0) for r in chosen) for name in {k for r in chosen for k in r['spans_ms']}}
            summaries.append(dict(tape=tape,console_frame_window=[first,last],host_probe_window=[first-1,last-1],frames=481,active_frames=sum(r['active'] for r in chosen),render_means=phases,span_means_ms=spans,binary_sha256=h.sha(h.BINS/'lg-host-attribution'),limits='Instrumented host scopes, nested and not a Vita estimate. Full profiler changes observer cost. Frame-limited attribution runs are not full-tape oracle credits. Host probe frame zero corresponds to console spike frame one.'))
        (out/'summary.json').write_text(json.dumps(summaries[-len(windows):],indent=2)+'\n')
    (h.P/'cpu-family.json').write_text(json.dumps(summaries,indent=2)+'\n')
    print(json.dumps(summaries,indent=2),flush=True)
if __name__=='__main__':main()
