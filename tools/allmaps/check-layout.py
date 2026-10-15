#!/usr/bin/env python3
"""Compile exact source struct declarations against package dependencies to read Vita buffer ABI sizes.
Emits a tiny relocatable object only; does not rebuild or modify the game/package.
"""
import argparse,hashlib,json,os,struct,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--target',type=Path,required=True);p.add_argument('--evidence',type=Path,required=True);a=p.parse_args()
R=Path(__file__).resolve().parents[2];E=a.evidence.resolve();E.mkdir(parents=True,exist_ok=True)
def declaration(source,name):
 start=source.index('struct '+name+' {');start=source.rfind('\n',0,start)+1
 end=source.index('\n}',start)+2
 return source[start:end]
renderer=(R/'src/render/vita_renderer.rs').read_text();shader=(R/'src/render/vita_renderer/shader.rs').read_text()
probe='use macroquad::{miniquad::TextureId,prelude::*};\nmod shader {\n#[repr(C)]\n'+declaration(shader,'Uniforms')+'\n}\n'+declaration(renderer,'Prepared')+'\n#[no_mangle]\npub static ALLMAPS_LAYOUT:[u32;4]=[std::mem::size_of::<Prepared>() as u32,std::mem::size_of::<Vertex>() as u32,std::mem::size_of::<Vec3>() as u32,std::mem::size_of::<usize>() as u32];\n'
(E/'layout-probe.rs').write_text(probe)
paths=sorted(a.target.resolve().rglob('libmacroquad-*.rmeta'));assert len(paths)==1,paths
paths_lib=sorted({p.parent for pattern in ['*.rmeta','*.dylib'] for p in a.target.resolve().parents[1].rglob(pattern)})
command=['rustc','--edition=2021','--crate-type=lib','--target=armv7-sony-vita-newlibeabihf','--emit=obj','-C','target-feature=-neon','--extern',f'macroquad={paths[0]}']
for crate in ['std','core','alloc','compiler_builtins']:
 libs=sorted(a.target.resolve().rglob('lib'+crate+'-*.rmeta'));assert len(libs)==1,libs
 command+=['--extern',f'{crate}={libs[0]}']
for path in paths_lib:command+=['-L',f'dependency={path}']
obj=E/'layout-probe.o';command += [str(E/'layout-probe.rs'),'-o',str(obj)]
assert not (Path.home()/'vita-j0/mac-timing.lock').exists(),'timing lock active'
with (E/'layout-probe.log').open('w') as log:result=subprocess.run(command,env=dict(os.environ,RUSTUP_TOOLCHAIN='nightly-2026-10-11'),stdout=log,stderr=subprocess.STDOUT,check=False)
assert result.returncode == 0, (E/'layout-probe.log').read_text()
assert obj.exists()
b=obj.read_bytes();assert b[:5]==b'\x7fELF\x01'
h=struct.unpack_from('<HHIIIIIHHHHHH',b,16);shoff,shsize,shcount=h[5],h[10],h[11]
sections=[struct.unpack_from('<IIIIIIIIII',b,shoff+i*shsize) for i in range(shcount)]
found=None
for s in sections:
 if s[1]!=2:continue
 strings=sections[s[6]];names=b[strings[4]:strings[4]+strings[5]]
 for at in range(s[4],s[4]+s[5],s[9]):
  name,value,size,_,_,index=struct.unpack_from('<IIIBBH',b,at)
  if names[name:].split(b'\0',1)[0]!=b'ALLMAPS_LAYOUT':continue
  section=sections[index];found=struct.unpack_from('<4I',b,section[4]+value)
assert found and found[3]==4
result=dict(zip(['prepared_bytes','vertex_bytes','vec3_bytes','usize_bytes'],found))
result.update(probe_sha256=hashlib.sha256(probe.encode()).hexdigest(),macroquad_rmeta=str(paths[0]),macroquad_rmeta_sha256=hashlib.sha256(paths[0].read_bytes()).hexdigest(),method='exact Prepared/Uniforms declarations extracted from source; real package macroquad types; native nightly Vita target -neon; ELF constant readback')
(E/'VITA-LAYOUT.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
