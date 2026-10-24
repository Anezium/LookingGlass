"""Compare r3 and r4 on the Mac using shared Cargo output, never transport."""
from pathlib import Path
import hashlib,io,json,os,subprocess,sys,tarfile
R=Path(__file__).resolve().parents[2];T=Path(__file__).resolve().parent
O=Path(sys.argv[1]).resolve();O.mkdir(exist_ok=True);(O/'logs').mkdir(exist_ok=True)
env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B',LOOKING_GLASS_TEST_DATA='/Users/anezium/vita-work/alice-data/Alice1/bin/base',LOOKING_GLASS_TEST_AUDIO_CONFIG=str(O.parent/'configs/deterministic-audio-config.json'))
results={}
for label,ref in [('r3','881988ec1801a68a425d43c04c63d8998372e952'),('r4','HEAD')]:
 S=O/label;S.mkdir(exist_ok=True)
 archive=subprocess.check_output(['git','-C',str(R),'archive',ref])
 with tarfile.open(fileobj=io.BytesIO(archive)) as tar:tar.extractall(S,filter='data')
 p=S/'src/collision.rs';p.write_text(p.read_text()+(T/'collision_probe.rs').read_text())
 p=S/'src/audio/offmain.rs';p.write_text(p.read_text()+(T/'allocation_probe.rs').read_text())
 p=S/'src/kernel_alloc.rs';s=p.read_text()
 s=s.replace('    fn count() {','''    thread_local! {static BYTES:Cell<i64>=const {Cell::new(0)};static GROSS:Cell<u64>=const {Cell::new(0)};}
    pub fn census() -> (u64,u64,i64) { (finish(), GROSS.with(Cell::get), BYTES.with(Cell::get)) }
    fn count(delta: i64, call: bool) {''')
 s=s.replace('n.set(n.get() + 1)', 'n.set(n.get() + u64::from(call))')
 s=s.replace('            if a.get() {','''            if a.get() {
                BYTES.with(|b|b.set(b.get()+delta));
                if delta>0 {GROSS.with(|b|b.set(b.get()+delta as u64));}''',1)
 # Replace calls only in the host allocator; target code is left alone.
 split=s.index('#[cfg(target_os = "vita")]');h=s[:split];v=s[split:]
 h=h.replace('count();\n            System.alloc(l)', 'count(l.size() as i64,true);\n            System.alloc(l)').replace('count();\n            System.alloc_zeroed(l)', 'count(l.size() as i64,true);\n            System.alloc_zeroed(l)').replace('count();\n            System.realloc(p, l, n)', 'count(n as i64-l.size() as i64,true);\n            System.realloc(p, l, n)').replace('            System.dealloc(p, l)', '            count(-(l.size() as i64),false);\n            System.dealloc(p, l)').replace('        CALLS.with(|n| n.set(0));','        CALLS.with(|n| n.set(0)); BYTES.with(|b|b.set(0)); GROSS.with(|b|b.set(0));')
 p.write_text(h+v+'\n#[cfg(not(target_os="vita"))]\npub(crate) fn census_begin(){imp::begin();}\n#[cfg(not(target_os="vita"))]\npub(crate) fn census_finish()->(u64,u64,i64){imp::census()}\n')
 os.utime(S/'src/main.rs',None)
 rows={}
 for tag,args in [('comparison',['r4_','--','--nocapture','--test-threads=1']),('map-audio',['audio::offmain_tests::map_mixed_output_is_sample_exact','--','--ignored','--nocapture','--test-threads=1'])]:
  log=O/'logs'/f'{label}-{tag}.log';cmd=['cargo','test','--release','--offline','--locked','--features','native_timing,frame_total_only']+args
  print(label,tag,flush=True)
  with log.open('w') as f:subprocess.run(cmd,cwd=S,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
  lines=log.read_text().splitlines()
  rows[tag]=[l for l in lines if 'R4_SWEEP ' in l or 'R4_ALLOCATION ' in l or l.startswith('AUDIO_OFFMAIN_EQUAL ')]
 results[label]=rows
 (O/'COMPARISON.json').write_text(json.dumps(results,indent=2)+'\n')
def tagged(label,tag):return [json.loads(l.split(tag+' ',1)[1]) for l in results[label]['comparison'] if tag+' ' in l]
assert tagged('r3','R4_SWEEP')==tagged('r4','R4_SWEEP')
assert results['r3']['map-audio']==results['r4']['map-audio']
for row in tagged('r4','R4_ALLOCATION'):
 assert row['begin']==[0,0,0],row
 assert row['channel'][2]==sum(row[k] for k in ['fixed_packet','fixed_queues','fixed_alive','fixed_stats']),row
results['sample_exact_vs_r3']=True;results['sweeps_bit_exact_vs_r3']=True
(O/'COMPARISON.json').write_text(json.dumps(results,indent=2)+'\n')
print('PASS sample-exact audio, randomized sweeps and zero-allocation begin',flush=True)
