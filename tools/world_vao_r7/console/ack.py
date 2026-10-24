import argparse

def main():
    parser=argparse.ArgumentParser(description='INSPECTION; acknowledge the pending image after visual inspection')
    parser.add_argument('arguments',nargs='*')
    parser.parse_args()
    import json,sys,datetime
    from pathlib import Path
    B=Path(__file__).resolve().parent
    p=B/'PENDING-IMAGE.json';v=json.loads(p.read_text());v.update(time=datetime.datetime.now().astimezone().isoformat(),inspection=sys.argv[1])
    with (B/'image-inspections.jsonl').open('a') as f:f.write(json.dumps(v)+'\n')
    if not (B/'baseline-home-inspection.json').exists():(B/'baseline-home-inspection.json').write_text(json.dumps(v,indent=2)+'\n')
    p.unlink()

if __name__=='__main__':
    main()
