#!/usr/bin/env python3
"""Write separate test configs, never edit the shipped baseline."""
import argparse,hashlib,json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--out',required=True,type=Path);p.add_argument('--name',default='fp-hatter2-01.lgtape');a=p.parse_args()
assert not a.out.exists(),'use a new output directory'
source=Path(__file__).resolve().parents[2]/'configs/PL-rc13-release-play04.json'
base=json.loads(source.read_text());a.out.mkdir(parents=True)
base.update(replay_record=a.name,replay_play='',replay_start='chapter:hatter2@normal',replay_fight_pilot=True,replay_check_every=30,replay_state_every=0,replay_stop_after=0)
file=a.out/'record-hatter2.json';file.write_text(json.dumps(base,indent=2)+'\n')
print(hashlib.sha256(file.read_bytes()).hexdigest(),file)
