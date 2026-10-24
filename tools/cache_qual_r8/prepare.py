"""Mac-only preparation. Copies code/configs only; never imports console helpers."""
from pathlib import Path
import json, shutil, sys
R=Path(__file__).resolve().parents[2]
T=R/'tools/cache_qual_r8'
OLD=Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-acoustic-cache-r6')
O=Path(sys.argv[1]).resolve()
def write(path,value):
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(json.dumps(value,indent=2)+'\n',encoding='utf-8')
for d in ['console','scripts','configs','logs','proof','packages/baseline','packages/cache']:
    (O/d).mkdir(parents=True,exist_ok=True)
names=['pilot.py','ack.py','host-log-receiver.py','preserve.py','baseline-r3.py','save-proof.py',
       'map_identity.py','verify-human-entry.py','verify-arm-self.py','restore-arm.py','restore-ledgers.py',
       'verify-self.py','restore-files.py','check-session-budget.py']
for name in names:
    text=(OLD/'console'/name).read_text()
    text=text.replace("['baseline','cache','cache-census']","['baseline','cache']")
    text=text.replace('vita-acoustic-cache-r6-20261024','vita-cache-qual-r8-20261024').replace('alice-acoustic-cache-r6','alice-cache-qual-r8')
    text=text.replace('Fresh r6','Fresh r8').replace('Fresh r6','Fresh r8')
    if name=='pilot.py':
        text=text.replace('from vita_ftp import Session','from vita_ftp import Session as _TransportSession\ndef Session(*args,**kwargs):\n owner()\n return _TransportSession(*args,**kwargs)')
        text=text.replace("def owner():", "def owner():\n assert json.loads((ROOT/'OWNERSHIP.json').read_text())['sole_console_owner'] is True\n assert json.loads((ROOT/'PREFLIGHT.json').read_text())['interface']=='en0'")
        text=text.replace("assert remote not in", "assert remote not in")
    if name=='host-log-receiver.py':
        text=text.replace('    import socketserver,datetime,json,os','    import pilot as p\n    p.owner()\n    import socketserver,datetime,json,os')
    if name=='baseline-r3.py':
        text=text.replace("    p.rec('baseline-paths.json',paths)","    paths += [p.R+'cache_qual_command.json',p.R+'frame_profile/cache-qual.json',p.R+'frame_profile/cache-qual.tmp']\n    paths=list(dict.fromkeys(paths))\n    p.rec('baseline-paths.json',paths)")
    if name=='restore-files.py':
        text=text.replace("assert d==p.R+'frame_profile/' and re.match", "assert d in [p.R+'saves/',p.R+'route-saves/'] or (d==p.R+'frame_profile/' and (n in ['cache-qual.json','cache-qual.tmp'] or re.match")
        text=text.replace(",n),('UNEXPECTED NEW FILE", ",n))),('UNEXPECTED NEW FILE")
        # Preserve every newly created save twice before deletion, just like exports.
        text=text.replace("    p.rec('RESTORE-FINAL.json'", "    index_before=(p.ROOT/'cache-index-before.json').read_bytes()\n    index_current=p.CACHE_INDEX.read_bytes()\n    (p.ROOT/'cache-index-before-final-restoration.json').write_bytes(index_current)\n    p.CACHE_INDEX.write_bytes(index_before)\n    assert p.CACHE_INDEX.read_bytes()==index_before\n    p.rec('CACHE-INDEX-RESTORATION.json',dict(status='PASS',before_sha256=p.sha(index_current),restored_sha256=p.sha(index_before)))\n    p.rec('RESTORE-FINAL.json'")
    (O/'console'/name).write_text(text,encoding='utf-8')
for name in ['log-integrity.py','test-log-integrity.py','verify-elf-margin.py','verify-process-params.py']:
    text=(OLD/'scripts'/name).read_text()
    if name=='test-log-integrity.py':
        text=text.replace("    protocol = json.loads((OUT / 'CONSOLE-PROTOCOL.json').read_text())", "    protocol = json.loads((OUT / 'CONSOLE-PROTOCOL.json').read_text())\n    template=protocol['sequence'][0]\n    protocol['sequence']=[dict(template,label=f'fixture-{i}-{arm}',arm=arm,session='fixture-main') for i,arm in enumerate('ABBAABBA',1)]")
    (O/'scripts'/name).write_text(text,encoding='utf-8')
shutil.copytree(OLD/'proof/log-integrity-fixtures',O/'proof/log-integrity-fixtures',dirs_exist_ok=True)
for name in ['bind_helpers.py']:
    text=(OLD/'scripts'/name).read_text().replace('session-s7-acoustic-cache','session-s8-cache-qual')
    (O/'scripts'/name).write_text(text,encoding='utf-8')
for path in (T/'console').glob('*.py'):
    shutil.copyfile(path,O/'console'/path.name)
for path in T.glob('*.py'):
    if path.name!='prepare.py':shutil.copyfile(path,O/'scripts'/path.name)
for name in ['CONSOLE-BRIEF.md','DESIGN.md','CONSOLE-REVIEW-PROCEDURE.md']+[p.name for p in T.glob('*AMENDMENT*.md')]:
    if (T/name).exists():shutil.copyfile(T/name,O/name)
for arm in ['baseline','cache']:
    cfg=json.loads((OLD/'configs/PL-baseline.json').read_text())
    cfg.update(audio_cache_qualification=True,audio_static_cache=arm=='cache',phase_census=False)
    write(O/'configs'/f'PL-{arm}.json',cfg)
for name in ['vita-sim-all.json','native-timing-disabled.json']:
    shutil.copyfile(OLD/'configs'/name,O/'configs'/name)
(O/'configs/profile_mode-full.txt').write_text('full\n',encoding='utf-8')
write(O/'configs/cache_qual_command.json',dict(id=0,action='stop',label='inactive'))
seq=[dict(label=f's8-cache-qual-{i}-{arm}',arm=arm,package=pkg,config_arm=pkg,session='s8-gameplay',
          map='hedge3',visit_key=None,entry='Chapters') for i,arm,pkg in [(1,'A','baseline'),(2,'B','cache')]]
write(O/'CONSOLE-PROTOCOL.json',dict(schema=1,sequence=seq,optional_sequence=[],session_directory='session-s8-cache-qual',
    windows=['hedge3-move','hedge3-check','pandemonium-move','pandemonium-check','reload-post','transition-post'],
    motion=dict(seconds=64,stick='left-stick',x=[64,192],y=128,pulse_seconds=2,repetitions=16,
        per_map={name:dict(axis='lateral',sequence=[[64,128,2],[192,128,2]]*16,
            evidence='AMENDMENTS-REVIEW.md R2: retained r6 entry images cannot establish two-second clearance; keep lateral') for name in ['hedge3','pandemonium']}),
    reading_rules=dict(mismatches=0,underruns=0,heap_limit_MiB=180,paired_heap_limit_MiB=1,
        event_absolute_heap='DESCRIPTIVE for transitions, loads and audio-load reset samples',
        entry_pose_windows=['hedge3-move','pandemonium-move','reload-post','transition-post'],
        check_stride={name:64 for name in ['hedge3-check','pandemonium-check','reload-post','transition-post']},
        native_operation_order=['save-reload cross-map','save-reload pandemonium same-map','chapter-or-console to hedge3'],
        load_interval_audio_gated=True,default_on_movement_mean_delta_limit_ms=.25,
        movement_heap_runtime_drift='UNKNOWN; new matched pair under a new amendment',
        require_inspected_modal_main_menu=True,require_title_closed_before_restore=True,
        require_warm_reset_at_every_load=True,hit_rates='DESCRIPTIVE',no_default_promotion_by_helpers=True)))
remote='ux0:/data/alice/private/'
for arm in ['baseline','cache']:
    files=[dict(local=f'configs/PL-{arm}.json',remote=remote+'vita_renderer.json',kind='small')]
    for local,dest in [('vita-sim-all.json','vita_sim.json'),('native-timing-disabled.json','native-timing-config.json'),
                       ('profile_mode-full.txt','profile_mode'),('cache_qual_command.json','cache_qual_command.json')]:
        files.append(dict(local='configs/'+local,remote=remote+dest,kind='small'))
    write(O/f'stage-{arm}-full.json',dict(files=files))
write(O/'stage-baseline-self.json',dict(files=[dict(local='packages/baseline/eboot.bin',remote='ux0:/app/ALICELG01/eboot.bin',kind='self')]))
print('Prepared r8 code/config package; no transport imports or device actions')
