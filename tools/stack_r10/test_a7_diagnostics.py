"""Schema 7 (A7): declared parents, merge/world/tls cross-checks, real Rust export."""
import copy,json,os,sys
from pathlib import Path
import test_r15_diagnostics as r15
mod=r15.mod

def export():
 outputs=Path(os.environ.get('ALICE_TIMING_TEST_OUTPUT','/nonexistent'))
 path=outputs/'a7-synthetic-census.json'
 if not path.exists():
  print('SKIP schema 7: set ALICE_TIMING_TEST_OUTPUT after running the a7_ Rust test');sys.exit(0)
 return path

def rejected(d,change,why):
 d=copy.deepcopy(d);change(d)
 try:mod.validate(d,d['map'])
 except AssertionError:return
 raise AssertionError('accepted invalid schema-7 document: '+why)

def set_parent(d,leaf,parent):
 for r in d['rows']:
  m=1<<parent;r['parent_masks'][leaf]=m&((1<<64)-1);r['parent_masks_hi'][leaf]=m>>64

def main():
 path=export();d=r15.check_export(path)
 assert d['schema']==7 and d['regions'][67:]==mod.A7_LEAVES and len(d['diagnostic_counter_columns'])==44
 result=mod.report(d)
 regions={r['region']:r for r in result['regions']}
 for i,parents in mod.A7_PARENTS.items():
  assert regions[d['regions'][i]]['parents']==[d['regions'][p] for p in parents],d['regions'][i]
 assert regions['actor_pose']['parents']==['gather_npcs']
 assert result['merge_bytes']['copied_vertex_bytes_per_frame']['mean']==200*d['vertex_bytes']
 assert result['tls']['counting'] is False
 assert '| joint_merge_copy |' in mod.markdown(result)
 rejected(d,lambda d:set_parent(d,69,d['parent_mask_frame_bit']),'merge leaf at frame root')
 rejected(d,lambda d:set_parent(d,73,14),'camera leaf under actor_gather')
 rejected(d,lambda d:d['rows'][0]['diagnostic_counters'].__setitem__(34,5),'merge plans without plan leaves')
 rejected(d,lambda d:d['rows'][0]['diagnostic_counters'].__setitem__(41,7),'tls counts while counting is off')
 rejected(d,lambda d:d['rows'][0]['calls'].__setitem__(84,d['rows'][0]['calls'][40]+1),'more between pairs than draws')
 rejected(d,lambda d:d.__setitem__('vertex_bytes',0),'missing vertex size')
 print('PASS schema 7: Rust export, 18 declared parents, gather nesting, merge bytes, 6 invalid mutants rejected')
if __name__=='__main__':main()
