"""Schema 6 closure, missing-work, wide-parent and real Rust export regressions."""
import copy,csv,json,os
from pathlib import Path
import test_r14b_diagnostics as old
mod=old.mod

def document():
 d=old.document();d.update(schema=6,map='hedge3',regions=mod.R15_REGIONS,parent_mask_frame_bit=67,parent_mask_words=2,diagnostic_counter_columns=mod.R15_COUNTERS,world_clocks_reused=False,world_clock_reads_per_draw=4,npc_query_controllers=['generic','levels','other'],npc_query_categories=['gravity_recoil','navigation_detour','sight_visibility','spawn_wave','other'],npc_query_columns=mod.NPC_COLUMNS,npc_repeat_rule='fixture')
 parents={52:0,53:0,54:18,55:18,56:18,57:18,58:56,59:1,60:1,61:59,62:21,63:21,64:62,65:49,66:49}
 for r in d['rows']:
  masks=[(m&~(1<<52))|((1<<67) if m&(1<<52) else 0) for m in r['parent_masks']]+[0]*15
  for field in ['inclusive_ns','exclusive_ns','calls']:r[field]+=[0]*15
  # The historical fixture has no NPC/player/HUD roots. Add them as real roots.
  for i in [0,1,18,21]:r['calls'][i]=1;masks[i]=1<<67;r['inclusive_ns'][i]=r['exclusive_ns'][i]=100
  r['calls'][2]=1;masks[2]=1<<1;r['inclusive_ns'][2]=r['exclusive_ns'][2]=100;r['inclusive_ns'][1]+=100
  for i,parent in reversed(list(parents.items())):
   r['calls'][i]=1;masks[i]=1<<parent;r['inclusive_ns'][i]+=100;r['exclusive_ns'][i]=100
   r['inclusive_ns'][parent]+=r['inclusive_ns'][i]
  # A parent introduced later in the descending loop is already credited for
  # its children. Propagate its entire inclusive duration only once.
  r['parent_masks']=[m&((1<<64)-1) for m in masks];r['parent_masks_hi']=[m>>64 for m in masks]
  r['diagnostic_counters']+=[0]*24;c=r['diagnostic_counters']
  for i in [17,18,19,24,25,27,28,29,30,31]:c[i]=1
  c[13]=64;c[15]=128;c[20]=2;c[21]=1;c[23]=2;c[26]=2
  q=[[0]*12 for _ in range(15)];q[0][0]=3;q[5][0]=1;q[0][1]=1;q[0][8]=r['sweep_ns'][2];q[0][9]=r['static_traversal_ns'][2]
  r['npc_query_categories']=q
  r['bracket_union_ns']=sum(r['exclusive_ns']);r['unbracketed_residual_ns']=r['frame_total_ns']-r['bracket_union_ns']
  r['reused_brackets']=0;r['added_brackets']=sum(r['calls'][30:])
 return d

def remove_leaf(d,leaf):
 r=d['rows'][0];m=mod.parent_masks(d,r)[leaf];parent=next(i for i in range(68) if m&(1<<i))
 r['exclusive_ns'][parent]+=r['exclusive_ns'][leaf]
 r['inclusive_ns'][leaf]=r['exclusive_ns'][leaf]=0
 if leaf>=30:r['added_brackets']-=r['calls'][leaf]
 r['calls'][leaf]=r['parent_masks'][leaf]=r['parent_masks_hi'][leaf]=0

def rejected(change):
 d=document();change(d)
 # This remains exactly closed, except for the explicit closure mutant.
 try:mod.validate(d,'hedge3')
 except AssertionError:return
 raise AssertionError('accepted invalid schema-6 frame')

def check_export(path):
 d=json.loads(path.read_text());mod.validate(d,d['map']);mod.report(d)
 with path.with_suffix('.csv').open(newline='') as f:rows=list(csv.DictReader(f))
 assert len(rows)==180
 for r,csvrow in zip(d['rows'],rows):
  for field in ['frame_total_ns','bracket_union_ns','unbracketed_residual_ns','added_brackets','reused_brackets']:assert int(csvrow[field])==r[field]
  for field,suffix in [('inclusive_ns','inclusive_ns'),('exclusive_ns','exclusive_ns'),('calls','calls'),('parent_masks','parent_mask'),('parent_masks_hi','parent_mask_hi')]:
   for i,name in enumerate(d['regions']):assert int(csvrow[name+'_'+suffix])==r[field][i]
  for i,name in enumerate(d['diagnostic_counter_columns']):assert int(csvrow[name])==r['diagnostic_counters'][i]
  for i,name in enumerate(d['publication_rejection_columns']):assert int(csvrow['publication_reject_'+name])==r['publication_rejections'][i]
  for i,b in enumerate(r['npc_query_categories']):
   for j,column in enumerate(d['npc_query_columns']):assert int(csvrow['npc_'+d['npc_query_controllers'][i//5]+'_'+d['npc_query_categories'][i%5]+'_'+column])==b[j]
 return d

def main():
 d=document();assert len(mod.validate(d,'hedge3'))==120
 for leaf in [2,40,41,42,*range(52,65)]:rejected(lambda d,leaf=leaf:remove_leaf(d,leaf))
 rejected(lambda d:d['rows'][0]['parent_masks_hi'].__setitem__(65,1<<6))
 rejected(lambda d:d['rows'][0]['npc_query_categories'][0].__setitem__(10,4))
 rejected(lambda d:d['rows'][0]['npc_query_categories'][0].__setitem__(9,2))
 rejected(lambda d:d['rows'][0].__setitem__('added_brackets',0))
 rejected(lambda d:d['rows'][0].__setitem__('unbracketed_residual_ns',0))
 outputs=Path(os.environ.get('ALICE_TIMING_TEST_OUTPUT','/nonexistent'))
 paths=[outputs/f'r15-{m}-census.json' for m in ['centipede1','utemple','wchess2','hatter1']]
 if all(p.exists() for p in paths):
  seen=set()
  for path in paths:
   exported=check_export(path)
   for r in exported['rows']:seen.update(i for i,n in enumerate(r['calls']) if n)
  assert set(range(52,67))<=seen
  print('PASS schema 6: four Rust exports, 720 rows, every new leaf, all CSV/JSON columns equal')
 print('PASS schema 6: exact closure, two-word parents, 17 missing-work and 5 integrity mutants rejected')
if __name__=='__main__':main()
