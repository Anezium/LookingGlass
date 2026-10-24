"""Archive only preparation code/config/docs in LG; never game assets/binaries."""
from pathlib import Path
import shutil
O=Path(__file__).resolve().parents[1]
R=Path('/Users/anezium/vita-work/lg-lever-matrix-r3')
dest=R/'tools/lever_matrix_r3'
dest.mkdir(exist_ok=True)
for name in ['console','configs','scripts']:
    shutil.copytree(O/name,dest/name,dirs_exist_ok=True,ignore=shutil.ignore_patterns('__pycache__','*.pyc'))
for source in [O/'DESIGN.md',O/'CONSOLE-BRIEF.md',O/'CONSOLE-PROTOCOL.json',*O.glob('stage-*.json')]:
    shutil.copyfile(source,dest/source.name)
(dest/'README.md').write_text('''# Alice matrix r3 preparation source

Authoritative runnable package: /Users/anezium/vita-j0/resume-2026-10-24/alice-lever-matrix-r3.
This directory archives its preparation source, amended console helpers and
runtime configs. Package manifests retain absolute paths to that sealed output.
Do not treat this source directory as a console session or run campaigns here.
No SELF, VPK, ELF, save, screenshot or original game asset is stored here.
The engine logging commit is recorded in the output VERIFICATION.json files.
All device operation requires the later ownership/admission workflow in the brief.
''')
print('Archived only code, runtime configs and documentation under tools/lever_matrix_r3')
