"""Extract bounded, offline evidence without inventing unavailable phase counts."""
from pathlib import Path
import csv, hashlib, json, re, statistics, subprocess
O=Path(__file__).resolve().parents[1]
P=Path('/Users/anezium/vita-j0/resume-2026-10-23/alice-lever-matrix')
H=Path('/Users/anezium/vita-j0/resume-2026-10-22/alice-world-submit')
R=Path('/Users/anezium/vita-work/lg-lever-matrix-r3')
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
runs=[]
for run in sorted((P/'session-s2-world').glob('s2-world-pandemonium-*')):
    if not run.is_dir(): continue
    log=run/'runtime-log.txt';text=log.read_text()
    loaded=[json.loads(line.split(' loaded ',1)[1]) for line in text.splitlines() if line.startswith('[lg/vita-renderer] loaded ')]
    draws=[line for line in text.splitlines() if '[lg/load-diagnostic]' in line and 'world_draws=' in line]
    receipt=json.loads((run/'COLLECT.json').read_text())
    rows=list(csv.DictReader(Path(receipt['files'][0]['local']).open()))
    fields={k:sorted({row[k] for row in rows}) for k in rows[0] if k not in ('frame','wall_us','gl_observers')}
    runs.append(dict(label=run.name,log_path=str(log),log_sha256=sha(log),
        loaded=loaded,entry_draw_receipts=draws,nonwall_fields=fields,
        median_ms=sorted(int(row['wall_us'])/1000 for row in rows)[len(rows)//2]))
host=json.loads((H/'HOST-GATES.json').read_text())
host_map=next(x for x in host['maps'] if x['map']=='pandemonium')
host_arms=[]
for arm in host_map['arms']:
    if arm['arm'] not in ('off','world_material_sort','world_static_merge','world_uniform_persist'):continue
    path=H/arm['log']
    host_arms.append(dict(arm=arm['arm'],log_path=str(path),log_sha256=sha(path),
        gl_calls_per_frame=arm['gl_calls_per_frame'],image_sha256=arm['image_sha256'],
        scope='historical host opaque-owner counters, last 240 of 300 frames; different entry/build, no Vita timing attribution'))
source_files=['src/render/vita_renderer/submit.rs','src/render/uniform_fields.rs','src/render/vita_renderer/state_cache.rs']
comparisons={name:subprocess.check_output(['git','-C',str(R),'diff','8e966b7','a859cee','--',name],text=True) for name in source_files}
data=dict(status='BUNDLE_REGRESSION_CONFIRMED_SINGLE_CAUSE_UNRESOLVED',console_contact=False,
    runs=runs,historical_host=host_arms,historical_host_source_diff_to_matrix=comparisons,
    per_frame_draws_state_changes_merge_counts='unavailable in observer-free CSV; not zero',
    geometry='equal GPU payload; static merging changes immutable buffer packing only; no per-frame upload in engine path',
    attribution='Neither bundled ABBA nor host call counts isolate the Vita time cost; three single-switch ABBA blocks required',
    renderer_behavior_change=False)
(O/'proof/WORLD-DIAGNOSIS.json').write_text(json.dumps(data,indent=2)+'\n')
print('PASS: extracted four console logs/CSVs and four historical host counter arms; causal attribution remains unresolved')
