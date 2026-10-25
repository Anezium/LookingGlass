"""Capture a visual launch's complete local log after inspected HOME; no device access."""
from pathlib import Path
import argparse,hashlib,json

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('label');a=p.parse_args()
 import pilot
 from run_health import entry_for_label
 assert entry_for_label(a.label).get('visual_only') is True
 run=pilot.ROOT/a.label;launch=json.loads((run/'LAUNCH.json').read_text())
 log=pilot.LOG.read_bytes();offset=launch['log_offset'];assert 0<=offset<len(log)
 raw=log[offset:]
 with (run/'runtime-log.txt').open('xb') as f:f.write(raw)
 (run/'LOG-RANGE.json').write_text(json.dumps(dict(start=offset,end=len(log),bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest(),source=str(pilot.LOG),visual_only=True),indent=2)+'\n')
 print('Captured full visual log; run the whole-log integrity scan next')
if __name__=='__main__':main()
