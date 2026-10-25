from pathlib import Path
import json,shutil,sys
R=Path(__file__).resolve().parents[2];T=R/'tools/stack_r10';O=Path(sys.argv[1]).resolve()
SWITCHES=('audio_static_cache','collision_sah','skin_actual_consumers')
for d in ['console','scripts','configs','proof']:(O/d).mkdir(parents=True,exist_ok=True)
for f in (T/'console').glob('*.py'):shutil.copy2(f,O/'console'/f.name)
for f in T.glob('*.py'):shutil.copy2(f,O/'scripts'/f.name)
for n in ['vita-sim-all.json','native-timing-disabled.json','profile_mode-full.txt','profile_mode-fullbench.txt']:shutil.copy2(R/'tools/lever_matrix_r4/configs'/n,O/'configs'/n)
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
base.update(collision_sah=False,audio_static_cache=False,skin_actual_consumers=False,phase_census=False)
def arm_config(stack,census):
    return dict(base,audio_static_cache=stack,collision_sah=stack,skin_actual_consumers=stack,phase_census=census)
for arm,stack,census in [('baseline',False,False),('stack',True,False),('baseline-census',False,True),('stack-census',True,True)]:
    (O/'configs'/f'PL-{arm}.json').write_text(json.dumps(arm_config(stack,census),indent=2)+'\n')
for name in ['log-integrity.py','test-log-integrity.py','verify-elf-margin.py','verify-process-params.py','capture-environment.py']:
    source=R/'tools/lever_matrix_r4/scripts'/name
    s=source.read_text().replace('STOP_ALL_REMAINING_R4','STOP_ALL_REMAINING_R10')
    if name=='log-integrity.py':
        s=s.replace("sequence=protocol['sequence'];","sequence=protocol['sequence']+protocol.get('optional_sequence',[])+protocol.get('smoke_sequence',[]);")
        s=s.replace("for e in protocol['sequence'])==1","for e in protocol['sequence']+protocol.get('optional_sequence',[])+protocol.get('smoke_sequence',[]))==1")
        s=s.replace('delta==1',"delta==protocol.get('config_record_A_minus_B_bytes',1)")
        s=s.replace('when both arms are present longest(A) - longest(B) = 1 byte',
                    'when both arms are present longest(A) - longest(B) equals protocol config_record_A_minus_B_bytes, default 1')
    if name=='test-log-integrity.py':
        s=s.replace("protocol = json.loads((OUT / 'CONSOLE-PROTOCOL.json').read_text())",
                    "protocol = json.loads((OUT / 'CONSOLE-PROTOCOL.json').read_text())\n    protocol.pop('config_record_A_minus_B_bytes', None)\n    extra=dict(protocol['sequence'][0],label='fixture-isolated-1-A',session='fixture-independent');protocol['sequence'].append(extra)")
        s=s.replace("real_a = a + '/runtime-log.txt'","real_a = 's4-audio-pandemonium-1-A/runtime-log.txt'").replace("real_b = b + '/runtime-log.txt'","real_b = 's4-audio-pandemonium-2-B/runtime-log.txt'")
    (O/'scripts'/name).write_text(s)
shutil.copytree(R/'tools/lever_matrix_r4/proof/log-integrity-fixtures',O/'proof/log-integrity-fixtures',dirs_exist_ok=True)
protocol=json.loads((R/'tools/lever_matrix_r4/CONSOLE-PROTOCOL.json').read_text());old=protocol['sequence'][:8];sequence=[];optional=[];smoke=[]
for name in ['hedge3','pandemonium']:
    for e in [x for x in old if x['map']==name]:
        e=dict(e,session='s10-stack',config_arm='stack' if e['arm']=='B' else 'baseline',package='stack' if e['arm']=='B' else 'baseline',census=False,smoke=False)
        e['block']=e['session']+'-'+name;e['label']=e['block']+'-'+e['label'].split('-')[-2]+'-'+e['arm'];sequence.append(e)
for arm in ['A','B']:
    e=dict(next(x for x in sequence if x['map']=='hedge3' and x['arm']==arm),session='s10-stack-census',census=True,config_arm='baseline-census' if arm=='A' else 'stack-census',package='baseline-census' if arm=='A' else 'stack-census')
    e['block']='s10-stack-census-hedge3';e['label']=e['block']+'-'+('5' if arm=='A' else '6')+'-'+arm;optional.append(e)
for name in ['hedge3','pandemonium']:
    e=dict(next(x for x in sequence if x['map']==name and x['arm']=='B'),session='s10-stack-load-smoke',smoke=True)
    e['block']=e['session']+'-'+name;e['label']=e['block']+'-0-B';smoke.append(e)
protocol.update(sequence=sequence,optional_sequence=optional,smoke_sequence=smoke,sessions=['s10-stack','s10-stack-load-smoke'],session_directory='session-s10-stack',smoke_session_directory='session-s10-stack-load-smoke',roles={'A':'audio_static_cache=false, collision_sah=false, skin_actual_consumers=false','B':'audio_static_cache=true, collision_sah=true, skin_actual_consumers=true'},config_changes={'baseline':[],'stack':list(SWITCHES),'baseline-census':['phase_census'],'stack-census':list(SWITCHES)+['phase_census']},config_record_A_minus_B_bytes=3,scope='Mac-only package; NOT EXECUTED',measurement='One common SELF. B-only load smoke on Hedge3 then Pandemonium with all three switches on, then wall ABBA on both maps and one Hedge3 census run per arm. A is all three off. B is all three on.',pass_rule=(T/'READING-RULES.md').read_text(),heap_budget=dict(expected_hedge3_retained_delta_MiB=3.23,expected_hedge3_planning_range_MiB=[3.0,6.5],oriented_delta_max_MiB=8,hedge3_plateau_exclusive_MiB=165,whole_run_stop_MiB=180),human_self_sha256='387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990',human_autosave_sha256='ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9',separate_idle_gains_ms={'hedge3_audio_r6':-4.7,'hedge3_sah_r8':-4.3,'pandemonium_sah_r8':-1.6,'pandemonium_skin_r7':-4.0},separate_gains_are_not_a_wall_prediction=True)
(O/'CONSOLE-PROTOCOL.json').write_text(json.dumps(protocol,indent=2)+'\n')
for arm in ['baseline','stack','baseline-census','stack-census']:
    for mode in ['full','self']:
        m=json.loads((R/'tools/lever_matrix_r4/manifests'/f'stage-baseline-{mode}.json').read_text())
        for f in m['files']:f['local']=f['local'].replace('@OUTPUT@',str(O)).replace('/packages/baseline/',f'/packages/{arm}/').replace('/PL-baseline.json',f'/PL-{arm}.json')
        (O/f'stage-{arm}-{mode}.json').write_text(json.dumps(m,indent=2)+'\n')
(O/'stage-fullbench-mode-only.json').write_text((R/'tools/lever_matrix_r4/manifests/stage-fullbench-mode-only.json').read_text().replace('@OUTPUT@',str(O)))
for name in ['CONTRACT.md','READING-RULES.md','CONSOLE-BRIEF.md','AMENDMENTS.md','AMENDMENTS-3.md','AMENDMENTS-R10.md','MEASUREMENT-LIMITS.md','README.md']:
    shutil.copy2(T/name,O/name)
print('Prepared package, no session directory or transport action')
