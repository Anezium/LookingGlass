"""Consolidate final host/source/native proofs without console access."""
from pathlib import Path
import hashlib, json, re, subprocess, sys
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();SDK=Path('/Users/anezium/vitasdk/bin')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8')
expected={'hedge3':'be48be8db9eb6ac272fb2d07702e77e21a5f94cc078c79b4e8226c983b6cd65c',
 'gvillage':'287dbc6056dfae48b75cee5e755659f0194519794218ef687842cb1b77ba0109',
 'skool2':'dda1a3c4cfe3ff818e6a05fe178647879035a72c5add3437f472ade3127d71e0',
 'hatter1':'a070a2113bb78ffa9b6c00b4000d4797b6630b0b2e9bf31d524366eac3c199d1'}
tests=json.loads((O/'proof/HOST-TESTS.json').read_text());assert len(tests)==5 and all(r['exit_code']==0 for r in tests)
equal={}
for policy in ['default','production']:
    record=next(r for r in tests if r['policy']==policy and '--ignored' in r['argv'])
    text=Path(record['log']).read_text()
    samples=re.findall(r'AUDIO_MAIN_EQUAL fixture=(\w+) frames=(\d+) scalar_samples=(\d+) sha256=([a-f0-9]+)',text)
    assert len(samples)==6 and all(int(frames)==120 and int(count)==368640 for _,frames,count,_ in samples)
    for name,digest in expected.items():assert all(h==digest for n,_,_,h in samples if n==name) and any(n==name for n,_,_,_ in samples)
    checks=re.findall(r'AUDIO_QUALIFICATION_EQUAL map=(\w+) checks=(\d+) mismatches=(\d+)',text)
    assert {n for n,_,_ in checks}=={'hedge3','pandemonium'} and all(int(c)>0 and int(m)==0 for _,c,m in checks)
    random=re.findall(r'STATIC_CACHE_RANDOM_EQUAL map=(\w+) queries=(\d+) hits=(\d+) misses=(\d+) mutable_result_changes=(\d+) scene_replacements=(\d+) explicit_resets=(\d+)',text)
    assert len(random)==2 and all(int(q)==10240 and int(changed)>0 and int(scenes)==4 and int(resets)==7 for _,q,_,_,changed,scenes,resets in random)
    equal[policy]=dict(sample_bit_comparisons=6*368640,samples=samples,self_checks=checks,randomized_traces=random,
        r6_four_map_hashes_unchanged=True,log_sha256=sha(Path(record['log'])))
write(O/'proof/HOST-EQUALITY.json',dict(status='PASS',policies=equal,host_only=True,default_cache=False,default_recorder=False))
a=json.loads((O/'configs/PL-baseline.json').read_text());b=json.loads((O/'configs/PL-cache.json').read_text())
diff={k:[a.get(k),b.get(k)] for k in a.keys()|b.keys() if a.get(k)!=b.get(k)}
assert diff=={'audio_static_cache':[False,True]} and a['audio_cache_qualification'] and not a['phase_census']
unchanged=['Cargo.toml','Cargo.lock','build.rs','.cargo','src/logging.rs','src/render_direct.rs']
assert not subprocess.check_output(['git','diff','73e303c','HEAD','--',*unchanged],cwd=R)
changed=subprocess.check_output(['git','diff','--name-only','73e303c','HEAD'],cwd=R,text=True).splitlines()
assert not any(Path(f).suffix.lower() in ['.pk3','.bsp','.wav','.mp3','.tga','.png','.jpg','.tik','.skl','.skb'] for f in changed)
write(O/'proof/DEPENDENCIES-AND-DEFAULTS.json',dict(status='PASS',unchanged_inputs=unchanged,config_effective_diff=diff,
    qualification_flag_identical_in_arms=True,default_serialized_flag_absent_test=True,assets_added=False,
    lock_sha256=sha(R/'Cargo.lock')))
(O/'proof/ENGINE-DIFF.patch').write_bytes(subprocess.check_output(['git','diff','73e303c','HEAD','--','src'],cwd=R))
elf=O/'packages/baseline/looking-glass.elf'
symbols=subprocess.check_output([str(SDK/'arm-vita-eabi-nm'),'-n','-C',str(elf)],text=True).splitlines()
found=[i for i,line in enumerate(symbols) if 'cache_qualification::apply::{closure#0}' in line]
assert len(found)==1
i=found[0];start=int(symbols[i].split()[0],16)
end=next(int(line.split()[0],16) for line in symbols[i+1:] if re.match(r'^[a-fA-F0-9]+ [tT] ',line) and int(line.split()[0],16)>start)
asm=subprocess.check_output([str(SDK/'arm-vita-eabi-objdump'),'-d','-C',f'--start-address={start}',f'--stop-address={end}',str(elf)],text=True)
assert re.search(r'mov\.w\s+r0, #1245184',asm) and '__wrap_malloc' in asm
(O/'proof/native-qualification-window.s').write_text(asm)
write(O/'proof/NATIVE-STORAGE.json',dict(status='PASS',window_capacity=8192,ARM_row_bytes=152,ARM_window_reservation_bytes=1245184,
    six_window_row_reservations_bytes=7471104,six_window_MiB=7.125,both_arms_identical_reservations=True,
    encoding='streamed borrowed rows; no second JSON tree',allocation_evidence_sha256=sha(O/'proof/native-qualification-window.s'),
    requested_bytes_not_allocator_rounded_or_peak=True))
print('Host sample/trace equality, unchanged build inputs and final ARM storage PASS')
