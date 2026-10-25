"""Require M4B's two process-kill replies and inspected HOME before writes."""
import json
import pilot as p

QUIT_COMMAND='quit ALICELG01'
CANNOT_QUIT='Error: cannot quit the app. Is the TITLEID correct?'

def closed_response(replies):
    assert isinstance(replies,list) and len(replies)==2, 'Title state unproven: two quit replies required'
    assert all(isinstance(raw,str) for raw in replies), 'Title state unproven: invalid quit reply'
    first,second=[raw.strip() for raw in replies]
    assert first in ['Quit.',CANNOT_QUIT], 'Title state unproven: unexpected first quit reply'
    assert second==CANNOT_QUIT, 'Title state unproven: second quit must report cannot quit'
    return replies

def record_closure(reason):
    # Invalidate any earlier HOME proof before attempting a new proof. Record
    # every reply before validation, including the expected cannot-quit error.
    (p.ROOT/'TITLE-CLOSED.json').unlink(missing_ok=True)
    receipt=dict(started=p.now(),reason=reason,command=QUIT_COMMAND,replies=[],status='UNPROVEN')
    old=p.B;p.B=p.ROOT
    try:
        for number in range(2):
            raw=p.cmd(QUIT_COMMAND)
            receipt['replies'].append(raw)
            p.rec('QUIT-CLOSURE.json',receipt)
            if number==0:
                assert isinstance(raw,str) and raw.strip() in ['Quit.',CANNOT_QUIT], 'Title state unproven: unexpected first quit reply'
        closed_response(receipt['replies'])
        receipt['status']='REPLIES_VERIFIED_HOME_REQUIRED'
        return receipt
    except Exception as error:
        receipt['failure']=str(error)
        raise
    finally:
        p.rec('QUIT-CLOSURE.json',receipt)
        p.event('closure-checks.jsonl',receipt)
        p.B=old

def failure_restore():
    return any(p.ROOT.rglob('STOP.json')) or any(p.ROOT.rglob('ABORT.json')) or (p.ROOT/'RESTORATION-STARTED.json').exists()

def require_closed(reason,final=False):
    p.owner()
    proof=p.ROOT/'TITLE-CLOSED.json'
    assert proof.exists(),'Title state unproven: record two quit replies and inspect HOME'
    receipt=json.loads(proof.read_text())
    assert receipt['state']=='home' and receipt['inspection'] and receipt['closure']['command']==QUIT_COMMAND
    image=p.Path(receipt['path']);assert p.sha(image.read_bytes())==receipt['sha256'],'HOME evidence changed'
    closed_response(receipt['closure']['replies'])
    # A successful kill here invalidates the earlier image: require a fresh
    # prove-closed and HOME acknowledgement before any restoration writes.
    check=record_closure(reason)
    assert check['replies'][0].strip()==CANNOT_QUIT, 'Title was still running: fresh closure proof and HOME inspection required'
    proof.write_text(json.dumps(receipt,indent=2)+'\n')
    old=p.B;p.B=p.ROOT
    try:
        p.event('restore-closure-checks.jsonl',dict(reason=reason,closure=check,home=receipt))
        if final and not (p.ROOT/'RESTORATION-STARTED.json').exists():
            p.rec('RESTORATION-STARTED.json',dict(time=p.now(),reason=reason,closure=check,home=receipt,never_in_game_quit=True))
    finally:p.B=old

def live_action():
    assert not (p.ROOT/'RESTORATION-STARTED.json').exists(),'Restoration started: never launch, send gameplay inputs, or use in-game Quit'
    (p.ROOT/'TITLE-CLOSED.json').unlink(missing_ok=True)
