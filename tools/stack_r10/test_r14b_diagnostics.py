"""r14b reused-clock accounting and independent overlay hierarchy regressions."""
import test_r14_diagnostics as old
mod=old.mod
def document():
 d=old.document();d.update(schema=5,regions=mod.R14B_REGIONS.copy(),parent_mask_frame_bit=52,diagnostic_counter_columns=mod.R14B_COUNTERS)
 for r in d['rows']:
  for key in ['inclusive_ns','exclusive_ns','calls','parent_masks']:r[key].append(0)
  r['parent_masks']=[(m&~(1<<51))|((1<<52) if m&(1<<51) else 0) for m in r['parent_masks']]
  r['calls'][40:43]=[507]*3
  r['diagnostic_counters'][3]=507
  r['reused_brackets']=1521
  r['added_brackets']=19
  r['calls'][51]=1;r['parent_masks'][51]=1<<29
  r['inclusive_ns'][51]=r['exclusive_ns'][51]=100
  r['inclusive_ns'][29]+=100;r['inclusive_ns'][14]+=100
  r['bracket_union_ns']+=100;r['unbracketed_residual_ns']-=100
  r['diagnostic_counters'].append(1)
 return d
def rejected(change):
 d=document();change(d)
 try:mod.validate(d,'hedge3')
 except AssertionError:return
 raise AssertionError('accepted invalid r14b frame')
def main():
 d=document();assert len(mod.validate(d,'hedge3'))==120
 result=mod.report(d,wall_delta_ms=1.0)
 for i in range(40,43):assert result['regions'][i]['attributable_events_per_frame']==0
 assert result['regions'][36]['attributable_events_per_frame']==1
 rejected(lambda d:d['rows'][0].__setitem__('reused_brackets',0))
 rejected(lambda d:d['rows'][0].__setitem__('added_brackets',1540))
 rejected(lambda d:d['rows'][0]['parent_masks'].__setitem__(51,1<<52))
 rejected(lambda d:d['rows'][0]['parent_masks'].__setitem__(51,1<<31))
 rejected(lambda d:d['rows'][0]['diagnostic_counters'].__setitem__(7,2))
 rejected(lambda d:d['rows'][0]['exclusive_ns'].__setitem__(51,101))
 d=document();d['world_clocks_reused']=False
 for r in d['rows']:r['reused_brackets']=0;r['added_brackets']=1540
 mod.validate(d,'hedge3')
 d['rows'][0]['calls'][40:43]=[0]*3
 d['rows'][0]['parent_masks'][40:43]=[0]*3
 d['rows'][0]['inclusive_ns'][40:43]=[0]*3
 d['rows'][0]['exclusive_ns'][40:43]=[0]*3
 try:mod.validate(d,'hedge3')
 except AssertionError:pass
 else:raise AssertionError('accepted missing world leaves')
 print('PASS r14b: 507 world draws, 1521 reused + 19 added intervals, overlay separation, 6 invalid mutants rejected')
if __name__=='__main__':main()
