"""Offline r3-1 whole-log integrity gate, run only after collection."""
from pathlib import Path
import argparse,hashlib,json,re

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('label');p.add_argument('--root',type=Path,required=True);a=p.parse_args()
 root=a.root.resolve();run=root/a.label
 protocol=json.loads((root.parent/'CONSOLE-PROTOCOL.json').read_text());assert sum(e['label']==a.label for e in protocol['sequence'])==1
 log=run/'runtime-log.txt';raw=log.read_bytes();lines=raw.splitlines()
 patterns={'tag_after_column_zero':rb'.+\[(lg|cpal|miniquad)/','orphan_field':rb'^ [a-z_]+='}
 matches={k:[{'line':i,'text':v.decode('utf-8',errors='replace')} for i,v in enumerate(lines,1) if re.search(pattern,v)] for k,pattern in patterns.items()}
 lengths=[len(v) for v in lines if v.startswith(b'[lg/vita-renderer] config ')]
 longest=max(lengths,default=0);prior={}
 for f in root.glob('*/LOG-INTEGRITY.json'):
  if f.parent==run:continue
  d=json.loads(f.read_text());prior[f.parent.name]=d['longest_renderer_config_record_bytes']
 passed=not any(matches.values()) and longest>0 and all(n==longest for n in prior.values())
 result={'status':'PASS' if passed else 'INVALID_LOG_INTEGRITY_STOP_ALL_REMAINING_R4','log_sha256':hashlib.sha256(raw).hexdigest(),'counts':{k:len(v) for k,v in matches.items()},'matches':matches,'renderer_config_record_bytes':lengths,'longest_renderer_config_record_bytes':longest,'prior_run_longest_bytes':prior,'rule':'r3-1 unchanged: complete whole-log scan; same longest config record byte length in every run; no reconstruction or exclusion'}
 (run/'LOG-INTEGRITY.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2));raise SystemExit(0 if passed else 2)
if __name__=='__main__':main()
