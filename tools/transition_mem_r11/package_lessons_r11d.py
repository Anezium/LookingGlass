"""Verify r11c remained read-only and C1-C3/D1-D5 survived the new package."""
from pathlib import Path
import hashlib, json, sys, zipfile
O = Path(sys.argv[1]).resolve()
old = O.with_name('alice-transition-mem-r11c')
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
original = json.loads((O / 'proof/R11C-READ-ONLY.json').read_text())
current = {str(p.relative_to(old)): dict(bytes=p.stat().st_size, sha256=sha(p))
           for p in sorted(old.rglob('*')) if p.is_file()}
assert current == original, 'r11c package was modified'
names = ['CONSOLE-BRIEF.md', 'CONSOLE-PROTOCOL.json', 'AMENDMENT-R11.md',
         'AMENDMENT-R11B.md', 'AMENDMENT-R11C.md', 'AMENDMENT-R11C-R3.md']
names += [f'configs/{p.name}' for p in (old / 'configs').iterdir() if p.is_file()]
names += [f'console/{p.name}' for p in (old / 'console').glob('*.py')]
rows = []
for name in names:
    before = (old / name).read_bytes()
    if name == 'console/preserve.py':
        before = before.replace(b'vita-transition-mem-r11c-20261025', b'vita-transition-mem-r11d-20261025').replace(
            b"package='alice-transition-mem-r11c'", b"package='alice-transition-mem-r11d'")
    assert before == (O / name).read_bytes(), name
    rows.append(dict(file=name, sha256=sha(O / name), identity_only_adaptation=name=='console/preserve.py'))
assert not any(p.name.startswith('session-') for p in O.iterdir())
with zipfile.ZipFile(O / 'CONSOLE-PACKAGE.zip') as z:
    assert all(z.read(n) == (O / n).read_bytes() for n in z.namelist())
    assert 'AMENDMENT-R11C-R3.md' in z.namelist() and 'AMENDMENT-R11D.md' in z.namelist()
    assert not any('session-' in n or '__pycache__' in n or n.lower().endswith(('.pk3','.bsp','.wav','.tga','.ftx')) for n in z.namelist())
(O / 'proof/PACKAGE-LESSONS-R11D.json').write_text(json.dumps(dict(status='PASS',
    r11c_original_all_files_unchanged=True, preserved=rows, zip_all_members_byte_equal=True,
    session_directories_absent=True, protocol_byte_equal=True,
    retry='UNAVAILABLE_NO_VERIFIED_SAFE_DEATH_ROUTE'), indent=2) + '\n')
print('PASS read-only r11c, preserved C1-C3/D1-D5 rules and ZIP equality')
