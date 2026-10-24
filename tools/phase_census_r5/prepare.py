"""Materialize the Mac-only phase census console draft. Never opens transport."""
from pathlib import Path
import json,shutil,sys
R=Path('/Users/anezium/vita-work/lg-phase-census-r5');T=R/'tools/phase_census_r5';OLD=R/'tools/lever_matrix_r4';O=Path(sys.argv[1]).resolve()
O.mkdir(parents=True,exist_ok=True)
for d in ['console','configs','scripts','proof/log-integrity-fixtures']:(O/d).mkdir(parents=True,exist_ok=True)
for f in (OLD/'console').glob('*.py'):shutil.copy2(f,O/'console'/f.name)
for name in ['PL-baseline.json','vita-sim-all.json','native-timing-disabled.json','profile_mode-full.txt','profile_mode-fullbench.txt']:shutil.copy2(OLD/'configs'/name,O/'configs'/name)
base=json.loads((O/'configs/PL-baseline.json').read_text());assert not base.get('phase_census',False)
c=dict(base,phase_census=True);(O/'configs/PL-census.json').write_text(json.dumps(c,indent=2)+'\n')
for name in ['log-integrity.py','test-log-integrity.py','verify-elf-margin.py','verify-process-params.py','capture-environment.py']:
 shutil.copy2(OLD/'scripts'/name,O/'scripts'/name)
shutil.copytree(OLD/'proof/log-integrity-fixtures',O/'proof/log-integrity-fixtures',dirs_exist_ok=True)
protocol=json.loads((OLD/'CONSOLE-PROTOCOL.json').read_text());seq=[]
for e in protocol['sequence'][:8]:
 e=dict(e);e['session']='s6-phase-census';e['block']='s6-phase-census-'+e['map'];e['label']=e['block']+'-'+e['label'].split('-')[-2]+'-'+e['arm'];e['config_arm']=e['package']='census' if e['arm']=='B' else 'baseline';seq.append(e)
protocol.update(sessions=['s6-phase-census'],sequence=seq,measurement='wall-only ABBA; B additionally exports fixed-storage census',roles={'A':'census disabled','B':'census enabled'},config_changes={'baseline':[],'census':['phase_census']},scope='r5 Mac-only draft, NOT EXECUTED',pass_rule='No optimization verdict. If either oriented B-A median or their mean exceeds 1 ms, label breakdown OBSERVER_PERTURBED; otherwise NO_RESOLVED_OBSERVER_COST. Never subtract observer cost from regions.')
(O/'CONSOLE-PROTOCOL.json').write_text(json.dumps(protocol,indent=2)+'\n')
for arm in ['baseline','census']:
 for mode in ['full','self']:
  m=json.loads((OLD/'manifests'/f'stage-baseline-{mode}.json').read_text());
  for f in m['files']:
   f['local']=f['local'].replace('@OUTPUT@',str(O)).replace('/packages/baseline/',f'/packages/{arm}/').replace('/PL-baseline.json',f'/PL-{arm}.json')
  (O/f'stage-{arm}-{mode}.json').write_text(json.dumps(m,indent=2)+'\n')
m=(OLD/'manifests/stage-fullbench-mode-only.json').read_text().replace('@OUTPUT@',str(O));(O/'stage-fullbench-mode-only.json').write_text(m)
p=O/'console/run_health.py';s=p.read_text().replace("('baseline', 'audio', 'world-static-merge')","('baseline', 'census')");s=s.replace("('baseline', 'audio', 'world-static-merge')", "('baseline', 'census')");p.write_text(s)
p=O/'console/session.py';s=p.read_text();needle="    a=p.snapshot('exports',[p.R+'frame_profile/'+n for n in names]);";assert needle in s;s=s.replace(needle,"    if entry_for_label(label,PACKAGE/'CONSOLE-PROTOCOL.json')['arm']=='B':names.append(f'{mapname}-fullbench-census-180.json')\n"+needle)
needle="    capture_log(label)";s=s.replace(needle,"    if len(a)==3:\n        from census_report import validate\n        validate(json.loads(Path(a[2]['local']).read_text()), mapname)\n        for earlier in ROOT.glob('*/COLLECT.json'):\n            files=json.loads(earlier.read_text())['files']\n            assert len(files)<3 or files[2]['sha256']!=a[2]['sha256'], 'Stale census export'\n"+needle);p.write_text(s)
p=O/'console/audit-run.py';s=p.read_text().replace(" assert expected['profiler']=='off'", " assert configs[0].get('phase_census',False)==(entry['arm']=='B')\n assert expected.get('phase_census',False)==(entry['arm']=='B')\n if entry['arm']=='B':\n  from census_report import validate\n  validate(json.loads(Path(coll['files'][2]['local']).read_text()),mapname)\n assert expected['profiler']=='off'");p.write_text(s)
p=O/'scripts/log-integrity.py';p.write_text(p.read_text().replace('STOP_ALL_REMAINING_R4','STOP_ALL_REMAINING_R5'))
p=O/'scripts/test-log-integrity.py';p.write_text(p.read_text().replace('STOP_ALL_REMAINING_R4','STOP_ALL_REMAINING_R5'))
for name in ['census_report.py','test_tools.py','bind_helpers.py']:
 if (T/name).exists():shutil.copy2(T/name,O/('console' if name=='census_report.py' else 'scripts')/name)
# Comparator keeps r4's evidence/pose/save/audio checks, replaces the lever verdict.
p=O/'console/compare-abba.py';s=p.read_text();a=s.index("    both_le=");b=s.index("    result=dict",a);s=s[:a]+"    status='OBSERVER_PERTURBED' if any(d>1000 for d in delta) or mean>1000 else 'NO_RESOLVED_OBSERVER_COST'\n    benefit='NOT_AN_OPTIMIZATION_TEST'\n"+s[b:];s=s.replace("measurement='begin-to-begin wall; all other profiler fields unavailable'","measurement='begin-to-begin wall; B census is diagnostic, nested and never observer-corrected'")
a=s.index("        next_action=(");b=s.index("\n    output=",a);s=s[:a]+"        next_action='Continue to Hedge3 if every evidence/audio/identity/save/heap gate passes and the unchanged-driver remainder fits. Observer cost alone labels the breakdown and does not stop the second map.')"+s[b:]
s=s.replace("sys.exit(0 if status=='PASS_WALL_GATE' else 2)","sys.exit(0)");p.write_text(s)
print('Prepared draft; no console calls')
# r4 historical log fixture paths are immutable; labels in the new protocol differ.
p=O/'scripts/test-log-integrity.py';s=p.read_text().replace("real_a = a + '/runtime-log.txt'", "real_a = 's4-audio-pandemonium-1-A/runtime-log.txt'").replace("real_b = b + '/runtime-log.txt'", "real_b = 's4-audio-pandemonium-2-B/runtime-log.txt'")
s=s.replace("protocol = json.loads((OUT / 'CONSOLE-PROTOCOL.json').read_text())", "protocol = json.loads((OUT / 'CONSOLE-PROTOCOL.json').read_text())\n    extra=dict(protocol['sequence'][0],label='fixture-isolated-1-A',session='fixture-independent');protocol['sequence'].append(extra)")
p.write_text(s)
for name in ['CONSOLE-BRIEF.md','DESIGN.md']:
 if (T/name).exists():shutil.copy2(T/name,O/name)
if (T/'make_report.py').exists():shutil.copy2(T/'make_report.py',O/'console/make-report.py')
