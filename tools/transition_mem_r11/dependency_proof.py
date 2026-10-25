"""Compare fork source files to the reviewed package and published crate."""
from pathlib import Path
import hashlib, io, json, os, subprocess, sys, tarfile
R=Path(__file__).resolve().parents[2]
O=Path(sys.argv[1]).resolve()
def sha(b): return hashlib.sha256(b).hexdigest()
crate=next((Path.home()/'.cargo/registry/cache').rglob('macroquad-0.4.14.crate'))
assert sha(crate.read_bytes())=='d2befbae373456143ef55aa93a73594d080adfb111dc32ec96a1123a3e4ff4ae'
rows=[]
with tarfile.open(crate) as t:
    vcs=json.load(t.extractfile('macroquad-0.4.14/.cargo_vcs_info.json'))['git']['sha1']
    assert vcs=='2c60f754b0c0e053f4c5b272fee696fb45e5aca8'
    for m in t.getmembers():
        if m.isfile() and m.name.startswith('macroquad-0.4.14/src/'):
            name=m.name.split('/',1)[1]
            published=t.extractfile(m).read()
            upstream=subprocess.check_output(['git','show',f'{vcs}:{name}'],cwd='/Users/anezium/vita-work/macroquad-material-r11c')
            assert published==upstream,name
            rows.append(dict(file=name,sha256=sha(published),equal=True))
proof=dict(status='PASS',package_sha256=sha(crate.read_bytes()),published_revision=vcs,published_source_files=rows,forks=[])
env=dict(os.environ,RUSTUP_TOOLCHAIN='stable',CARGO_NET_OFFLINE='true')
metadata=json.loads(subprocess.check_output(['/Users/anezium/.cargo/bin/cargo','metadata','--locked','--offline','--format-version','1'],cwd=R,env=env))
for name,allowed in [('macroquad',['src/texture.rs','src/lib.rs']),('miniquad',['src/graphics/gl.rs'])]:
    fork=Path('/Users/anezium/vita-work')/('macroquad-material-r11c' if name=='macroquad' else 'miniquad-texture-r11c')
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=fork)
    consumed=Path(next(p['manifest_path'] for p in metadata['packages'] if p['name']==name)).parent
    tracked=subprocess.check_output(['git','ls-files','src','build.rs','LICENSE-MIT','LICENSE-APACHE','VITA-CHANGES.md'],cwd=fork,text=True).splitlines()
    files=[]
    for file in tracked:
        reviewed=subprocess.run(['git','show',f'aec5590:vendor/{name}/{file}'],cwd=R,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
        assert reviewed.returncode==0,file
        actual=(fork/file).read_bytes()
        assert (consumed/file).read_bytes()==actual,(name,file,'Cargo source differs from fork')
        equal=actual==reviewed.stdout
        assert equal or file in allowed,(name,file)
        files.append(dict(file=file,reviewed_sha256=sha(reviewed.stdout),fork_sha256=sha(actual),equal=equal))
    proof['forks'].append(dict(name=name,url=f'https://github.com/Anezium/{name}',revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=fork,text=True).strip(),cargo_source=str(consumed),cargo_source_byte_equal=True,files=files,exceptions=allowed))
(O/'proof/DEPENDENCY-BYTE-EQUALITY.json').write_text(json.dumps(proof,indent=2)+'\n')
print('PASS published source identity and file-by-file reviewed fork equality')
