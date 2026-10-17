#!/usr/bin/env python3
"""Mac-only step-1 checks with cooperative lock and frozen binary. No console IO."""
import argparse, hashlib, json, os, shutil, subprocess, time, select
from pathlib import Path
P=argparse.ArgumentParser()
P.add_argument('--evidence',type=Path,required=True)
P.add_argument('--phase',choices=['suites','allmaps','parity'],required=True)
a=P.parse_args()
R=Path(__file__).resolve().parents[1]; E=a.evidence.resolve(); E.mkdir(parents=True,exist_ok=True)
REF=Path.home()/'vita-j0/atoz/reference-build-r3f'; OLD=Path.home()/'vita-j0/atoz/threading/step0-r3f'
D=Path('/Users/anezium/vita-work/alice-data/Alice1/bin/base'); LOCK=Path.home()/'vita-j0/mac-timing.lock'
TARGET=Path.home()/'vita-j0/cargo-target-host'; B=E/'host/looking-glass'
ENV={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
ENV.update(CARGO_BUILD_JOBS='4',CARGO_TARGET_DIR=str(TARGET),PATH=str(Path.home()/'.cargo/bin')+os.pathsep+ENV['PATH'])
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def save(name,value):(E/name).write_text(json.dumps(value,indent=2)+'\n')
def run(label,command,cwd=R,extra=None,timeout=1800):
    start=time.monotonic()
    while True:
        try:
            fd=os.open(LOCK,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
            os.write(fd,json.dumps(dict(pid=os.getpid(),task='threading-step1',label=label,functional_only=True)).encode());os.close(fd);break
        except FileExistsError:
            if time.monotonic()-start>10800:raise TimeoutError('timing lock unavailable')
            print('Waiting for Mac lock:',label,flush=True)
            try: watch=os.open(LOCK,os.O_RDONLY)
            except FileNotFoundError: continue
            queue=select.kqueue()
            try:
                event=select.kevent(watch,filter=select.KQ_FILTER_VNODE,flags=select.KQ_EV_ADD|select.KQ_EV_ONESHOT,fflags=select.KQ_NOTE_DELETE|select.KQ_NOTE_RENAME)
                queue.control([event],1,30)
            finally: queue.close();os.close(watch)
    log=E/(label+'.log');log.parent.mkdir(parents=True,exist_ok=True)
    try:
        with log.open('w') as out:
            result=subprocess.run(list(map(str,command)),cwd=cwd,env=dict(ENV,**(extra or {})),stdout=out,stderr=subprocess.STDOUT,timeout=timeout)
        assert result.returncode==0,(label,result.returncode,log.read_text()[-5000:])
        # Freeze the shared-target output before releasing the lock.
        if label=='host-build':B.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(TARGET/'debug/looking-glass',B)
    finally:LOCK.unlink()
    print('PASS',label,flush=True);return log

def cell(name,arm,cfg=None,probe=False):
    c=E/'host'/name/arm;(c/'private').mkdir(parents=True,exist_ok=True)
    cfg=dict(cfg or json.loads((REF/'configs/r3f-candidate.json').read_text()),world_prepare_worker=arm=='on',sim_span_probe=probe)
    (c/'private/vita_renderer.json').write_text(json.dumps(cfg,indent=2)+'\n')
    shutil.copy2(REF/'configs/vita-sim-all.json',c/'private/vita_sim.json')
    return c

def hashes(log):return [json.loads(s.split(' ',1)[1]) for s in log.read_text().splitlines() if s.startswith('WORLD_PREPARE_HASH ')]

if a.phase=='suites':
    results=[]
    for label,cmd,extra in [
        ('tests-all',['cargo','+stable','test','--locked','--all-features'],{}),
        ('tests-all-sim-on',['cargo','+stable','test','--locked','--all-features'],{'LOOKING_GLASS_TEST_SIM_SWITCHES':'on'}),
        ('tests-default',['cargo','+stable','test','--locked'],{}),
        ('tests-fan',['cargo','+stable','test','--locked','--all-features','tower2_fan_models_match','--','--ignored','--nocapture'],{'LOOKING_GLASS_TEST_DATA':str(D)}),
        ('report-tests',['python3','tools/test_threading_step0_report.py'],{}),
        ('source-audit',['python3','tools/check_source.py','--worktree'],{}),
        ('confinement',['python3','tools/check_gl_confinement.py'],{}),
        ('host-build',['cargo','+stable','build','--locked','--features','vita_renderer,sim_maps_probe'],{})]:
        log=run(label,cmd,extra=extra);results.append(dict(label=label,log_sha256=sha(log),exit_code=0));save('SUITES.json',results)
    save('HOST-BINARY.json',dict(sha256=sha(B),cargo_lock_sha256=sha(R/'Cargo.lock')))
elif a.phase=='allmaps':
    visits=json.loads((REF/'ALLMAPS-VISITS.json').read_text());assert len(visits)==39
    baseline=json.loads((REF/'allmaps/IMAGE-DIFFERENCES.json').read_text())
    old={v['label']:v for v in baseline['visits']}
    results=dict(binary_sha256=sha(B),maps=36,visits=[],console_access=False)
    metrics=dict(LOOKING_GLASS_RENDER_METRICS='1',LOOKING_GLASS_WORLD_HASHES='1',LOOKING_GLASS_HOST_CLOCK_STEP='0')
    for visit in visits:
        row=dict(label=visit['label'],map=visit['map'],qualities={})
        for quality in ['full','low']:
            cfg=json.loads((REF/'configs/PL-parity-r2.json').read_text())
            cfg.update(renderer='vita',retained_maps=['*'],legacy_maps=[])
            if quality=='full':cfg.update(quality='full',render_scale=1.0)
            pair=[]
            for arm in ['off','on']:
                c=cell('allmaps/'+visit['label']+'/'+quality,arm,cfg)
                cmd=[B,'--data',D,'--map',visit['map'],'--allmaps-entry-check','--no-audio','--frames',3,'--capture',c/'entry.png']
                if visit['entry']:cmd+=['--entry',visit['entry']]
                log=run(str(c.relative_to(E))+'/run',cmd,cwd=c,extra=metrics,timeout=600)
                record=json.loads((c/'private/allmaps-entry.json').read_text());assert record['renderer']=='vita' and record['report']['construction_failures']==0
                h=hashes(log);assert len(h)>=6,(visit,arm,len(h))
                counts=record['world_prepare'];assert counts['world_prepare_jobs']>0 if arm=='on' else counts['world_prepare_jobs']==0
                assert counts['world_prepare_fallbacks']==0
                pair.append(dict(png_sha256=sha(c/'entry.png'),hashes=h,world_prepare=counts,stats=record['stats']))
            assert pair[0]['png_sha256']==pair[1]['png_sha256'],(visit,quality,'image parity')
            assert pair[0]['hashes']==pair[1]['hashes'],(visit,quality,'visibility/parameter parity')
            assert pair[0]['stats']==pair[1]['stats'],(visit,quality,'draw stats')
            assert pair[0]['png_sha256']==old[visit['label']]['comparisons'][quality]['retained']['image_sha256'],(visit,quality,'r3f image parity')
            row['qualities'][quality]=dict(image_equal=True,visibility_and_parameter_bits_equal=True,r3f_image_equal=True,arms=pair)
        results['visits'].append(row);save('ALLMAPS-PARITY.json',results)
else:
    results=dict(binary_sha256=sha(B),simulation=[],images=[])
    metrics=dict(LOOKING_GLASS_RENDER_METRICS='1',LOOKING_GLASS_WORLD_HASHES='1',LOOKING_GLASS_HOST_CLOCK_STEP='0')
    for name in ['skool2','hedge3','tower2','funhouse','hatter1','potears3','fortress1','gvillage','skool1','pandemonium']:
        pair=[];n=1200 if name=='funhouse' else 300
        for arm in ['off','on']:
            c=cell('sim-'+name,arm,probe=True)
            log=run(str(c.relative_to(E))+'/run',[B,'--data',D,'--map',name,'--no-audio','--frames',n,'--save-dir',c/'private/saves'],cwd=c,
                extra=dict(metrics,LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT='0.05',LOOKING_GLASS_SIM_MAPS_COUNTERS='1'))
            rows=[json.loads(s.split(' ',1)[1]) for s in log.read_text().splitlines() if s.startswith('SIM_MAPS_FRAME ')]
            assert len(rows)==n,(name,arm,len(rows));pair.append((rows,hashes(log)))
        assert [r['state_sha256'] for r in pair[0][0]]==[r['state_sha256'] for r in pair[1][0]],(name,'state')
        assert [r['counters'] for r in pair[0][0]]==[r['counters'] for r in pair[1][0]],(name,'workload')
        assert pair[0][1]==pair[1][1],(name,'numeric output')
        results['simulation'].append(dict(map=name,frames_per_arm=n,state_equal=True,counters_equal=True,world_hashes_equal=True));save('PARITY.json',results)
    for mode in ['renderer-host-check','skin-overlap-render-check']:
        pair=[]
        for arm in ['off','on']:
            c=cell(mode,arm,json.loads((REF/'configs/PL-parity-r2.json').read_text()))
            saves=c/'private/saves';saves.mkdir(exist_ok=True)
            shutil.copy2(Path.home()/'vita-j0/village-retained-world/jalon-23-bls12/mac-timing/inputs/auto.json',saves/'auto.json')
            log=run(str(c.relative_to(E))+'/run',[B,'--data',D,'--'+mode,'--no-audio'],cwd=c,extra=dict(metrics,LOOKING_GLASS_HOST_CLOCK_STEP='0.016666667'))
            pair.append(dict(images={str(p.relative_to(c)):sha(p) for p in c.rglob('*.png')},hashes=hashes(log)))
        assert pair[0]==pair[1],mode
        if mode=='renderer-host-check':
            c=cell(mode,'r3f',json.loads((REF/'configs/PL-parity-r2.json').read_text()))
            path=c/'private/vita_renderer.json';cfg=json.loads(path.read_text());del cfg['world_prepare_worker'];path.write_text(json.dumps(cfg,indent=2)+'\n')
            saves=c/'private/saves';saves.mkdir(exist_ok=True)
            shutil.copy2(Path.home()/'vita-j0/village-retained-world/jalon-23-bls12/mac-timing/inputs/auto.json',saves/'auto.json')
            run(str(c.relative_to(E))+'/run',[OLD/'host/looking-glass','--data',D,'--'+mode,'--no-audio'],cwd=c,extra=dict(metrics,LOOKING_GLASS_HOST_CLOCK_STEP='0.016666667'))
            reference={str(p.relative_to(c)):sha(p) for p in c.rglob('*.png')}
            assert reference==pair[0]['images'],'advancing-clock pre-extraction image parity'
            results['advancing_clock_pre_extraction_r3f']=dict(binary_sha256=sha(OLD/'host/looking-glass'),pngs=len(reference),byte_equal=True)
        results['images'].append(dict(mode=mode,pngs_per_arm=len(pair[0]['images']),images_and_hashes_equal=True,hashes=pair[0]));save('PARITY.json',results)
    print('PASS fixed-dt simulation and advancing-clock render parity',flush=True)
