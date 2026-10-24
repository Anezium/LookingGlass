"""Prepare build tooling for the six new packages; no transport."""
from pathlib import Path
O=Path(__file__).resolve().parents[1]
p=O/'scripts/build-matrix.py'
s=p.read_text().replace("ROOT=Path('/Users/anezium/vita-work/lg-lever-matrix')", "ROOT=Path('/Users/anezium/vita-work/lg-lever-matrix-r3')")
s=s.replace("for arm in ['baseline','world','fx','audio']:", "for arm in ['baseline','fx','audio','world-material-sort','world-static-merge','world-uniform-persist']:")
s=s.replace("run('host-world'", "run('host-world'")
s=s.replace("(OUT/'tmp').mkdir", "run('host-logging',['cargo','test','--offline','--locked','--features','frame_total_only','logging_','--','--nocapture'],host)\nrun('host-submission',['cargo','test','--offline','--locked','--features','frame_total_only','render::vita_renderer::submit::tests','--','--nocapture'],host)\n(OUT/'tmp').mkdir")
s=s.replace("Four packages built", "Six packages built")
p.write_text(s)

# Runtime/audit policy stays strict. A world-single-arm stage still exercises
# the real save restore path separately; this fixture targets config staging.
p=O/'scripts/dry-run-console.py';s=p.read_text()
s=s.replace("for entry in [next(x for x in sequence if x['config_arm']==arm) for arm", "for original_entry in [next(x for x in sequence if x['config_arm']==arm) for arm")
s=s.replace("            arm=entry['arm'];manifest=", "            entry=dict(original_entry, map='hedge3')\n            arm=entry['arm'];manifest=")
s=s.replace("patch.object(c,'stage_manifest',lambda m,l:json.loads(m.read_text())),patch.object(p,'snapshot',config_snapshot)", "patch.object(c,'stage_manifest',lambda m,l:json.loads(m.read_text())),patch.object(c,'entry_for_label',return_value=entry),patch.object(p,'snapshot',config_snapshot)")
p.write_text(s)
