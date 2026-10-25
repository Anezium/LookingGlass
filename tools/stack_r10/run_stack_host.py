"""All-off versus all-on host proofs. Never contacts a console or driver."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys,time
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve()
def write(p,v):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(v,indent=2)+'\n')
env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
env.update(PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+env['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B',CARGO_INCREMENTAL='0',CARGO_NET_OFFLINE='true')
cmd=['cargo','build','--release','--offline','--locked','--features','native_timing,frame_total_only,sim_maps_probe']
with (O/'logs/host-build.log').open('w') as f:subprocess.run(cmd,cwd=R,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
binary=O/'host/probe-bin';binary.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(Path(env['CARGO_TARGET_DIR'])/'release/looking-glass',binary)
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
base.update(audio_music_thread=False,phase_census=False,profile_capture=False,audio_static_cache=False,collision_sah=False,skin_actual_consumers=False)
keys=['state_sha256','selected_lights_bits','joint_alpha_order','blend_world_order','npc_decisions']
proof={};receipts=[]
for name,dt,frames in [('hedge3','0.060',1),('hedge3','0.060',180),('pandemonium','0.04184',180)]:
    rows=[]
    for arm,on in [('A',False),('B',True)]:
        run=O/'host'/f'{name}-{frames}-{arm}';(run/'private').mkdir(parents=True,exist_ok=True)
        cfg=dict(base,audio_static_cache=on,collision_sah=on,skin_actual_consumers=on)
        write(run/'private/vita_renderer.json',cfg)
        e=dict(env,LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT=dt)
        argv=[str(binary),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--map',name,'--no-audio','--frames',str(frames),'--save-dir',str(run/'private/saves')]
        if frames>1:argv+=['--capture',str(run/'frame.png')]
        start=time.monotonic()
        with (run/'run.log').open('w') as f:result=subprocess.run(argv,cwd=run,env=e,stdout=f,stderr=subprocess.STDOUT,timeout=900)
        log=(run/'run.log').read_text(errors='replace')
        loads=[line for line in log.splitlines() if line.startswith('COLLISION_SAH_LOAD ')]
        receipts.append(dict(map=name,frames=frames,arm=arm,exit_code=result.returncode,seconds=time.monotonic()-start,sah_loads=loads,binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest()))
        write(O/'proof/HOST-STACK-RUNS.json',receipts)
        assert result.returncode==0,(name,arm,frames)
        if on:
            assert len(loads)==1,(name,loads)
            assert 'hulls='+{'hedge3':'108424','pandemonium':'8211'}[name] in loads[0]
            if name=='hedge3':
                assert log.count('[lg/collision] footing source=shared_static')==1
                assert 'footing source=legacy_tree' not in log
        else:
            assert loads==[]
        if frames==180:
            frames_json=[json.loads(x.split('SIM_MAPS_FRAME ',1)[1]) for x in log.splitlines() if x.startswith('SIM_MAPS_FRAME ')]
            assert len(frames_json)==180
            rows.append(frames_json)
        print(name,frames,arm,'PASS',loads,flush=True)
    if frames==180:
        assert all(all(a[k]==b[k] for k in keys) for a,b in zip(*rows)),name
        proof[name]=dict(frames=180,all_equal=True,fields=keys,switches=['audio_static_cache','collision_sah','skin_actual_consumers'])
        write(O/'proof/STACK-SIMULATION-EQUALITY.json',proof)
print('PASS all-on versus all-off simulation hashes and a single Hedge3 SAH build')
