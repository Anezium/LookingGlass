"""Offline r4-1 whole-log integrity gate, run only after collection."""
from pathlib import Path
import argparse,hashlib,json,re

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('label');p.add_argument('--root',type=Path,required=True);a=p.parse_args()
 root=a.root.resolve();run=root/a.label
 protocol=json.loads((root.parent/'CONSOLE-PROTOCOL.json').read_text());assert sum(e['label']==a.label for e in protocol['sequence'])==1
 sequence=protocol['sequence'];index=next(i for i,e in enumerate(sequence) if e['label']==a.label);entry=sequence[index]
 def arm(label):
  assert label.endswith(('-A','-B')), 'Protocol label must end in -A or -B'
  return label[-1]
 current_arm=arm(a.label)
 log=run/'runtime-log.txt';raw=log.read_bytes();lines=raw.splitlines()
 patterns={'tag_after_column_zero':rb'.+\[(lg|cpal|miniquad)/','orphan_field':rb'^ [a-z_]+='}
 matches={k:[{'line':i,'text':v.decode('utf-8',errors='replace')} for i,v in enumerate(lines,1) if re.search(pattern,v)] for k,pattern in patterns.items()}
 lengths=[len(v) for v in lines if v.startswith(b'[lg/vita-renderer] config ')]
 longest=max(lengths,default=0);prior={};session_prior={};by_arm={'A':[],'B':[]};by_arm[current_arm].append(longest)
 for previous in sequence[:index]:
  if previous['session']!=entry['session']:continue
  label=previous['label'];f=root/label/'LOG-INTEGRITY.json'
  if not f.exists():continue
  n=json.loads(f.read_text())['longest_renderer_config_record_bytes'];previous_arm=arm(label)
  session_prior[label]=n;by_arm[previous_arm].append(n)
  if previous_arm==current_arm:prior[label]=n
 arm_longest={key:max(values) for key,values in by_arm.items() if values}
 delta=arm_longest['A']-arm_longest['B'] if len(arm_longest)==2 else None
 passed=not any(matches.values()) and longest>0 and all(len(set(values))<=1 for values in by_arm.values()) and (delta is None or delta==1)
 result={'status':'PASS' if passed else 'INVALID_LOG_INTEGRITY_STOP_ALL_REMAINING_R4','log_sha256':hashlib.sha256(raw).hexdigest(),'counts':{k:len(v) for k,v in matches.items()},'matches':matches,'renderer_config_record_bytes':lengths,'longest_renderer_config_record_bytes':longest,'arm':current_arm,'session':entry['session'],'prior_run_longest_bytes':prior,'prior_session_run_longest_bytes':session_prior,'session_arm_longest_bytes':arm_longest,'A_minus_B_bytes':delta,'rule':'r4-1: complete whole-log scan; equal longest config record byte length within each arm of this session, identified by protocol label suffix -A/-B; when both arms are present longest(A) - longest(B) = 1 byte; no reconstruction or exclusion'}
 (run/'LOG-INTEGRITY.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2));raise SystemExit(0 if passed else 2)
if __name__=='__main__':main()
