import argparse

def main():
    parser=argparse.ArgumentParser(description='INSPECTION; acknowledge the pending image after visual inspection')
    parser.add_argument('arguments',nargs='*')
    parser.add_argument('--main-menu',action='store_true',help='Inspected Main menu with Circle Back/Resume visible')
    parser.add_argument('--home',action='store_true',help='Inspected HOME/LiveArea showing no running Alice after the two-quit closure proof')
    args=parser.parse_args()
    assert len(args.arguments)==1 and args.arguments[0].strip() and not (args.main_menu and args.home)
    import json,sys,datetime
    from pathlib import Path
    B=Path(__file__).resolve().parent
    import hashlib
    p=B/'PENDING-IMAGE.json';v=json.loads(p.read_text());v.update(time=datetime.datetime.now().astimezone().isoformat(),inspection=args.arguments[0])
    v['sha256']=hashlib.sha256(Path(v['path']).read_bytes()).hexdigest()
    if args.main_menu:
        assert v.get('state')=='main-menu-candidate','Not a Start-menu image; do not infer modal state from the paused overlay'
        v.update(state='main-menu',circle_back_resume_confirmed=True)
    elif args.home:
        from title_safety import closed_response, QUIT_COMMAND
        assert v.get('state')=='home-candidate'
        assert v['closure']['command']==QUIT_COMMAND
        closed_response(v['closure']['replies']);v['state']='home'
        (B/'TITLE-CLOSED.json').write_text(json.dumps(v,indent=2)+'\n')
    with (B/'image-inspections.jsonl').open('a') as f:f.write(json.dumps(v)+'\n')
    if not (B/'baseline-home-inspection.json').exists():(B/'baseline-home-inspection.json').write_text(json.dumps(v,indent=2)+'\n')
    p.unlink()

if __name__=='__main__':
    main()
