"""Create the new offline package from the amended helpers, never session data."""
from pathlib import Path
import json, shutil

O = Path(__file__).resolve().parents[1]
P = Path('/Users/anezium/vita-j0/resume-2026-10-23/alice-lever-matrix')
R = Path('/Users/anezium/vita-work/lg-lever-matrix-r3')
arms = {'baseline': [], 'fx': ['fx_cpu_inline'], 'audio': ['audio_offmain'],
        'world-material-sort': ['world_material_sort'],
        'world-static-merge': ['world_static_merge'],
        'world-uniform-persist': ['world_uniform_persist']}

def write(path, data):
    path.write_text(json.dumps(data, indent=2)+'\n', encoding='utf-8')

for directory in ['console', 'configs']:
    shutil.copytree(P/directory, O/directory, dirs_exist_ok=True)
for name in ['dry-run-console.py', 'test-console-tools.py', 'test-matrix-contracts.py',
             'verify-elf-margin.py', 'verify-process-params.py', 'build-matrix.py', 'qualify-matrix.py']:
    shutil.copyfile(P/'scripts'/name, O/'scripts'/name)
protocol = json.loads((P/'CONSOLE-PROTOCOL.json').read_text())
protocol['sequence'] = [e for e in protocol['sequence'] if e['session'] in ('s3-fx', 's4-audio')]
template = json.loads((P/'CONSOLE-PROTOCOL.json').read_text())['sequence'][:4]
for arm in list(arms)[3:]:
    session = 's5-'+arm
    for i, old in enumerate(template, 1):
        entry = dict(old, session=session, block=session+'-pandemonium', label=f'{session}-pandemonium-{i}-{old["arm"]}',
                     config_arm='baseline' if old['arm']=='A' else arm, package='baseline' if old['arm']=='A' else arm)
        protocol['sequence'].append(entry)
protocol['config_changes'] = arms
protocol['sessions'] = list(dict.fromkeys(e['session'] for e in protocol['sequence']))
protocol['new_self_requires_fresh_A_baselines'] = True
protocol['historical_runs_admitted'] = False
write(O/'CONSOLE-PROTOCOL.json', protocol)
base = json.loads((O/'configs/PL-baseline.json').read_text())
(O/'configs/PL-world.json').unlink()
for arm, keys in arms.items():
    if arm not in ('baseline', 'fx', 'audio'):
        config = dict(base, **{key: True for key in keys})
        write(O/'configs'/f'PL-{arm}.json', config)
    (O/'packages'/arm).mkdir(parents=True, exist_ok=True)
    for mode in ['self', 'full']:
        manifest = json.loads((P/f'stage-baseline-{mode}.json').read_text())
        for f in manifest['files']:
            f['local'] = f['local'].replace(str(P), str(O))
            if mode == 'self':
                f['local'] = str(O/'packages'/arm/'eboot.bin')
            elif f['remote'].endswith('vita_renderer.json'):
                f['local'] = str(O/'configs'/f'PL-{arm}.json')
        write(O/f'stage-{arm}-{mode}.json', manifest)
manifest = json.loads((P/'stage-fullbench-mode-only.json').read_text())
for f in manifest['files']: f['local'] = f['local'].replace(str(P), str(O))
write(O/'stage-fullbench-mode-only.json', manifest)

# Keep audit policy and restoration bytes unchanged; extend exact identity enums.
for name in ['run_health.py', 'preserve.py']:
    path = O/'console'/name
    text = path.read_text().replace("('baseline','world','fx','audio')", repr(tuple(arms)))
    text = text.replace("['baseline','world','fx','audio']", repr(list(arms)))
    path.write_text(text)
path = O/'console/compare-abba.py'
text = path.read_text().replace("seq[0]['map']=='pandemonium' and status!='FAIL_WALL_GATE'", "seq[0]['map']=='pandemonium' and seq[0]['session'] in ('s3-fx','s4-audio') and status!='FAIL_WALL_GATE'")
path.write_text(text)
for name in ['dry-run-console.py', 'test-console-tools.py', 'test-matrix-contracts.py']:
    path = O/'scripts'/name
    text = path.read_text().replace('s2-world-hedge3','s3-fx-hedge3').replace('s2-world-pandemonium','s3-fx-pandemonium')
    text = text.replace("['baseline','world','fx','audio']", repr(list(arms)))
    text = text.replace("('baseline','world','fx','audio')", repr(tuple(arms)))
    text = text.replace('stage-world-full.json', 'stage-world-static-merge-full.json')
    # Single-world sessions have no Hedge3 entry. Test every arm on its actual map.
    text = text.replace("x['config_arm']==arm and x['map']=='hedge3'", "x['config_arm']==arm")
    path.write_text(text)

# Record the current local metadata only; do not check its process or call it.
ready = Path('/Users/anezium/vita-j0/resume-2026-10-21/timing-and-exit-followup/driver-v5/DRIVER-READY.json')
write(O/'proof/DRIVER-READY-SNAPSHOT.json', dict(source=str(ready), receipt=json.loads(ready.read_text()),
      purpose='read-only historical context; not admission, readiness or ownership', driver_contact=False))
print('Prepared six runtime configurations and 28 PENDING launches; no session state copied.')

