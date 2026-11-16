"""Full optimized unit checks without LTO; production oracle/SELF keep their recipes."""
import json,os,subprocess,shutil,time
import host_oracle as h
import runtime_equivalence as e
def main():
    assert shutil.disk_usage('/Users').free>41*2**30
    head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=h.SOURCE,text=True).strip()
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=h.SOURCE)
    native=json.loads((h.P/'builds/9da106a/BUILD-RECEIPT.json').read_text())
    e.verify(native['head'],head)
    target=os.environ.get('WCHESS_HOST_TARGET_DIR','/Users/anezium/vita-j0/cargo-target-host')
    profile=dict(CARGO_PROFILE_RELEASE_LTO='false',CARGO_PROFILE_RELEASE_CODEGEN_UNITS='256')
    output=h.P/'unit-output'/('check-'+head[:7]+'-'+str(time.time_ns()))
    output.mkdir(parents=True,exist_ok=False)
    env=dict(os.environ,ALICE_TIMING_TEST_OUTPUT=str(output),PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin',CARGO_TARGET_DIR=target,CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='4',RUSTFLAGS='--cfg observer_free_B',RUSTUP_TOOLCHAIN='nightly-2026-10-11-aarch64-apple-darwin',**profile)
    os.utime(h.SOURCE/'src/main.rs')
    with (h.P/'logs/unit.log').open('w') as out:
        subprocess.run(['cargo','test','--release','--offline','--locked','--features','native_timing,vita_renderer,frame_total_only,sim_maps_probe','--','--test-threads=1'],cwd=h.SOURCE,env=env,stdout=out,stderr=subprocess.STDOUT,check=True)
    assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=h.SOURCE,text=True).strip()==head
    (h.P/'unit-receipt.json').write_text(json.dumps(dict(head=head,result=0,target=target,output=str(output),profile_overrides=profile),indent=2)+'\n')
    print('UNIT PASS',head,flush=True)
    assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=h.SOURCE,text=True).strip()==head
    with (h.P/'logs/feature-off.log').open('w') as out:
        subprocess.run(['cargo','check','--offline','--locked'],cwd=h.SOURCE,env=env,stdout=out,stderr=subprocess.STDOUT,check=True)
    (h.P/'feature-off-receipt.json').write_text(json.dumps(dict(head=head,result=0),indent=2)+'\n')
    print('FEATURE OFF PASS',flush=True)
if __name__=='__main__':main()
