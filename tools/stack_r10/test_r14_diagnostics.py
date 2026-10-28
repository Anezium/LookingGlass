"""Synthetic r14 partition, hierarchy, CSV pairing and rejection fixtures."""
import copy,csv,io,json,sys
from pathlib import Path
import test_census_closure as old
mod=old.mod
def document():
 d=old.document(False,[[0,0,0]]*3);d.update(schema=4,regions=mod.R14_REGIONS.copy(),parent_mask_frame_bit=51,publication_rejection_columns=mod.R14_REJECTIONS,diagnostic_counter_columns=mod.R14_COUNTERS)
 for r in d['rows']:
  r.update(inclusive_ns=[0]*51,exclusive_ns=[0]*51,calls=[0]*51,parent_masks=[0]*51,publication_rejections=[1]*11,diagnostic_counters=[20,10,1234,507,31,812,222],added_brackets=21)
  parents={14:51,29:14,11:51,19:51,30:29,31:29,32:29,33:19,34:19,35:19,36:35,37:35,38:19,39:19,40:11,41:11,42:11}
  parents.update({i:51 for i in range(43,51)})
  for i,parent in parents.items():
   r['exclusive_ns'][i]=1000;r['calls'][i]=1;r['parent_masks'][i]=1<<parent
  for i in sorted(parents,reverse=True):
   r['inclusive_ns'][i]+=r['exclusive_ns'][i]
   if parents[i]!=51:r['inclusive_ns'][parents[i]]+=r['inclusive_ns'][i]
  # Root parents have smaller indexes than all their descendants.
  r['bracket_union_ns']=sum(r['exclusive_ns']);r['unbracketed_residual_ns']=r['frame_total_ns']-r['bracket_union_ns']
 return d
def rejected(change):
 d=document();change(d)
 try:mod.validate(d,'hedge3')
 except AssertionError:return
 raise AssertionError('accepted invalid diagnostic frame')
def main():
 d=document();assert len(mod.validate(d,'hedge3'))==120
 rejected(lambda d:d['rows'][0]['exclusive_ns'].__setitem__(30,1001))
 rejected(lambda d:d['rows'][0]['parent_masks'].__setitem__(30,1<<51))
 rejected(lambda d:d['rows'][0]['parent_masks'].__setitem__(36,1<<19))
 rejected(lambda d:d['rows'][0]['parent_masks'].__setitem__(50,1<<19))
 rejected(lambda d:d['rows'][0].__setitem__('added_brackets',0))
 rejected(lambda d:d['rows'][0]['diagnostic_counters'].__setitem__(1,21))
 rejected(lambda d:d['rows'][0]['publication_rejections'].pop())
 rejected(lambda d:d['rows'][0]['calls'].__setitem__(30,0))
 rejected(lambda d:d['regions'].__setitem__(30,'typo'))
 print('PASS r14: every new leaf, exact closure, wide hierarchy, all counters; 9 invalid mutants rejected')
if __name__=='__main__':main()
