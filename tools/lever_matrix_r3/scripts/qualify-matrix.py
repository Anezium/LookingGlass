"""Verify the new SELF, exact source/config delta, hooks and native timer."""
from pathlib import Path
import hashlib, json, re, struct, subprocess
O=Path(__file__).resolve().parents[1]
R=Path('/Users/anezium/vita-work/lg-lever-matrix-r3')
P=Path('/Users/anezium/vita-j0/resume-2026-10-23/alice-lever-matrix')
SDK=Path('/Users/anezium/vitasdk/bin')
OLD='a859ceea2e35fdffec97872bc9cea46956ccbbd2'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def git(*a):return subprocess.check_output(['git','-C',str(R),*a],text=True).strip()
def write(name,data):(O/'proof'/name).write_text(json.dumps(data,indent=2)+'\n')
def sections(path):
    data=path.read_bytes(); assert data[:5]==b'\x7fELF\x01'
    h=struct.unpack_from('<16sHHIIIIIHHHHHH',data)
    entries=[struct.unpack_from('<10I',data,h[6]+i*h[11]) for i in range(h[12])]
    names=entries[h[13]];strings=data[names[4]:names[4]+names[5]]
    return {strings[e[0]:].split(b'\0',1)[0].decode():dict(address=e[3],offset=e[4],size=e[5],type=e[1],
        sha256=hashlib.sha256(data[e[4]:e[4]+e[5]] if e[1]!=8 else b'').hexdigest()) for e in entries}

changed=git('diff','--name-only',OLD,'HEAD','--','src','Cargo.toml','Cargo.lock','build.rs','.cargo').splitlines()
assert changed==['src/logging.rs','src/main.rs'],changed
(O/'proof/SOURCE-DIFF.patch').write_text(git('diff',OLD,'HEAD','--','src','Cargo.toml','Cargo.lock','build.rs','.cargo')+'\n')
assert not git('diff',OLD,'HEAD','--','Cargo.lock','Cargo.toml','build.rs','.cargo','docs/RCHESS1.md')
assert (O/'console/strict-audio.py').read_bytes()==(P/'console/strict-audio.py').read_bytes()
for name in ['src/frame_total_profile.rs','src/render_direct.rs','src/frame_total_gl_names.txt']:
    assert sha(R/name)==sha(Path('/Users/anezium/vita-work/lg-lever-matrix')/name)
manifest={p:sha(R/p) for p in git('ls-files','src','Cargo.toml','Cargo.lock','build.rs','.cargo').splitlines()}
write('SOURCE-MANIFEST.json',dict(build_revision=json.loads((O/'packages/baseline/VERIFICATION.json').read_text())['revision'],
    files=manifest,engine_changed_files=changed,baseline_config_sha256=sha(O/'configs/PL-baseline.json'),
    source_diff_sha256=sha(O/'proof/SOURCE-DIFF.patch'),unchanged_dependency_pins=True,
    config_off_scope='all engine source except logger/module declaration identical; runtime on new SELF unmeasured'))
protocol=json.loads((O/'CONSOLE-PROTOCOL.json').read_text());arms=protocol['config_changes']
base=json.loads((O/'configs/PL-baseline.json').read_text())
assert (O/'configs/PL-baseline.json').read_bytes()==(P/'configs/PL-baseline.json').read_bytes()
assert len(protocol['sequence'])==28 and all(e['status']=='PENDING' for e in protocol['sequence'])
assert protocol['sessions']==list(dict.fromkeys(e['session'] for e in protocol['sequence'])) and len(protocol['sessions'])==5
proof={};packages=[]
for arm,keys in arms.items():
    package=O/'packages'/arm;v=json.loads((package/'VERIFICATION.json').read_text())
    config=json.loads((O/'configs'/f'PL-{arm}.json').read_text())
    changes={k:dict(baseline=base[k],arm=config[k]) for k in base if base[k]!=config[k]}
    assert set(changes)==set(keys) and len(changes)<=1
    assert sha(package/'eboot.bin')==v['self_sha256']
    assert not any(config[k] for k in ['light_gl_observers','world_binding_cache','world_gl_state_cache','fx_merge_ranges','sim_dynamic_buckets','render_draw_timing','frame_phase_probe','worker_probe'])
    assert config['profiler']=='off' and config['profile_capture']
    for mode in ['self','full']:
        files=json.loads((O/f'stage-{arm}-{mode}.json').read_text())['files']
        if mode=='self':assert len(files)==1 and files[0]['remote']=='ux0:/app/ALICELG01/eboot.bin'
        else:assert {f['remote'] for f in files}=={'ux0:/data/alice/private/'+n for n in ['vita_renderer.json','vita_sim.json','native-timing-config.json','profile_mode']}
        assert all(Path(f['local']).is_file() and Path(f['local']).is_absolute() for f in files)
    proof[arm]=dict(config_sha256=sha(O/'configs'/f'PL-{arm}.json'),exact_diff=changes);packages.append(v)
assert len({v['self_sha256'] for v in packages})==1
assert packages[0]['self_sha256']!=sha(P/'packages/baseline/eboot.bin')
write('CONFIG-ARM-DIFFS.json',proof)
symbols=(O/'logs/symbols.log').read_text()
assert not any('looking_glass::render_tail::hooks' in l for l in symbols.splitlines())
assert 'tested 29 names: zero GL observer hooks' in (O/'logs/host-frame-total.log').read_text()
assert '3 passed; 0 failed' in (O/'logs/host-frame-total.log').read_text()
def disasm(path,name,filename):
    text=subprocess.check_output([str(SDK/'arm-vita-eabi-objdump'),'-d','-C','--disassemble='+name,str(path)],text=True)
    (O/'proof'/filename).write_text(text);rows=[]
    for l in text.splitlines():
        m=re.match(r'\s*([0-9a-f]+):\s+((?:(?:[0-9a-f]{8}|[0-9a-f]{4})\s+)+)\s*([a-z][a-z0-9.]*)\s*(.*)',l)
        if m:rows.append(dict(address=m[1],encoding=m[2].strip(),mnemonic=m[3],operands=m[4]))
    assert rows;return rows
newelf=O/'packages/baseline/looking-glass.elf';oldelf=P/'packages/baseline/looking-glass.elf'
def symbol_map(text):
    result={}
    for l in text.splitlines():
        m=re.match(r'([0-9a-f]{8}) [a-zA-Z] (.*)',l)
        if m:result.setdefault(int(m[1],16),[]).append(m[2])
    return result
newmap=symbol_map(symbols);oldmap=symbol_map((P/'logs/symbols.log').read_text());timer={}
def sized_symbols(elf):
    text=subprocess.check_output([str(SDK/'arm-vita-eabi-nm'),'-a','-S',str(elf)],text=True)
    return {int(m[1],16):int(m[2],16) for line in text.splitlines() if (m:=re.match(r'([0-9a-f]{8}) ([0-9a-f]{8}) ',line))}
oldsize=sized_symbols(oldelf);newsize=sized_symbols(newelf)
oldsections=sections(oldelf);newsections=sections(newelf)
oldbytes=oldelf.read_bytes();newbytes=newelf.read_bytes()
def read_virtual(data,secs,address,size):
    section=next(s for s in secs.values() if s['type']!=8 and s['address']<=address and address+size<=s['address']+s['size'])
    offset=section['offset']+address-section['address'];return data[offset:offset+size]
def anonymous_equal(old_address,new_address):
    size=oldsize[old_address];assert size==newsize[new_address]
    a=read_virtual(oldbytes,oldsections,old_address,size);b=read_virtual(newbytes,newsections,new_address,size)
    if a==b:return dict(kind='identical constant',bytes=size,sha256=hashlib.sha256(a).hexdigest())
    if size==16 and a[4:12]==b[4:12]:
        relocations=[]
        for offset in (0,12):
            op=struct.unpack_from('<I',a,offset)[0];np=struct.unpack_from('<I',b,offset)[0]
            shared=set(oldmap.get(op & ~1,[])) & set(newmap.get(np & ~1,[]))
            if not shared:break
            relocations.append(dict(offset=offset,symbols=sorted(shared)))
        if len(relocations)==2:return dict(kind='identical vtable size/alignment and relocated methods',bytes=size,relocations=relocations)
    # Rust panic Location is { filename pointer, filename length, line, column }.
    assert size==16 and a[4:]==b[4:],(hex(old_address),hex(new_address),a,b)
    op,length,line,column=struct.unpack('<4I',a);np=struct.unpack_from('<I',b)[0]
    oldfile=read_virtual(oldbytes,oldsections,op,length);newfile=read_virtual(newbytes,newsections,np,length)
    assert oldfile==newfile
    return dict(kind='identical panic location except filename pointer relocation',filename=oldfile.decode(),line=line,column=column,bytes=size)
for name in ['alice_frame_total_begin','alice_frame_total_sample']:
    a=disasm(oldelf,name,'old-'+name+'.asm');b=disasm(newelf,name,name+'.asm')
    assert [r['mnemonic'] for r in a]==[r['mnemonic'] for r in b],name
    pointer_changes=[]
    for i,(old,new) in enumerate(zip(a,b)):
        def norm(s):return re.sub(r'\b[0-9a-f]{8}\s+(<.*>)',r'\1',s.split('@',1)[0].strip())
        x=norm(old['operands']);y=norm(new['operands'])
        if x!=y:
            assert old['mnemonic'] in ('movw','movt'),(name,i,old,new)
            register=old['operands'].split(',')[0]
            low=i if old['mnemonic']=='movw' else next(j for j in range(i-1,max(-1,i-5),-1) if a[j]['mnemonic']=='movw' and a[j]['operands'].startswith(register+','))
            high=next(j for j in range(low+1,min(len(a),low+5)) if a[j]['mnemonic']=='movt' and a[j]['operands'].startswith(register+','))
            def pointer(rows):
                lo=re.match(r'(r\d+), #(\d+)',rows[low]['operands']);hi=re.match(r'(r\d+), #(\d+)',rows[high]['operands'])
                assert lo and hi and lo[1]==hi[1]
                return lo[1],int(lo[2])+(int(hi[2])<<16)
            reg,op=pointer(a);nreg,np=pointer(b);assert reg==nreg
            shared=set(oldmap.get(op,[])) & set(newmap.get(np,[]))
            constant=None
            if not shared:constant=anonymous_equal(op,np)
            pointer_changes.append(dict(instruction=i,old_address=hex(op),new_address=hex(np),symbols=sorted(shared),anonymous_constant=constant))
    calls=[r['operands'] for r in b if r['mnemonic'] in ('bl','blx')]
    if name=='alice_frame_total_begin':assert sum('::now>' in call for call in calls)==1
    timer[name]=dict(instructions=len(b),same_mnemonics=True,same_operands_except_symbol_relocations=True,pointer_relocations=pointer_changes,calls=calls)
write('ZERO-GL-HOOKS.json',dict(status='PASS',observer_hooks=0,resolver_names_tested=29,native_hook_symbols=[],
    direct_resolver_source_identical=True,timer_source_sha256=sha(R/'src/frame_total_profile.rs'),native_timer=timer,
    all_arm_executables_byte_identical=True,identical_timer_scope='identical source and native operands except verified symbol/call/branch address relocation'))
oldsections=sections(oldelf);newsections=sections(newelf)
write('SELF-COMPARISON.json',dict(old_self_sha256=sha(P/'packages/baseline/eboot.bin'),new_self_sha256=sha(O/'packages/baseline/eboot.bin'),
    old_self_bytes=(P/'packages/baseline/eboot.bin').stat().st_size,new_self_bytes=(O/'packages/baseline/eboot.bin').stat().st_size,
    sections={n:dict(old=oldsections.get(n),new=newsections.get(n),identical=oldsections.get(n)==newsections.get(n)) for n in sorted(oldsections.keys()|newsections.keys())},
    explanation='Common logging macros consolidate formatting code, changing link/LTO layout, rodata, unwind tables, symbols and relocations. SELF header/segment metadata follows the new layout. Not every changed SELF byte range can be individually attributed; the complete source diff is authoritative.',
    source_diff='proof/SOURCE-DIFF.patch',unchanged_engine_except_logging=True,dependency_lock_sha256=sha(R/'Cargo.lock'),
    runtime_parity='UNMEASURED; fresh new-SELF A baseline mandatory'))
for v in packages:
    v.update(logging_fix=True,engine_sources_identical_except_logging=True,old_self_sha256=sha(P/'packages/baseline/eboot.bin'),
        source_diff='proof/SOURCE-DIFF.patch',source_diff_sha256=sha(O/'proof/SOURCE-DIFF.patch'),new_self_requires_fresh_A=True,
        identical_timer='same native timer instructions except verified symbol address relocation; same executable across new arms',
        hook_proof='proof/ZERO-GL-HOOKS.json',strict_audio_parser_unchanged=True)
    (O/'packages'/v['arm']/'VERIFICATION.json').write_text(json.dumps(v,indent=2)+'\n')
(O/'PACKAGE-MATRIX.json').write_text(json.dumps(dict(status='BUILT_HOST_QUALIFIED_RUNTIME_UNMEASURED',packages=packages,
    common_self_sha256=packages[0]['self_sha256'],console_access=False,driver_contact=False),indent=2)+'\n')
print('PASS: six single-switch packages; logging-only engine diff; zero hooks; identical native timer modulo verified relocation')
