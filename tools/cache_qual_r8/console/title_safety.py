"""Require explicit title closure and inspected HOME before restoration writes."""
import json, re
import pilot as p

def closed_response(raw):
    # Accept only an explicit state for this title, never an error, empty result,
    # generic HOME state or an unrecognized command response.
    assert isinstance(raw,str) and 'ALICELG01' in raw and 'error' not in raw.lower(), 'Title state unproven; ask the human to close ALICELG01 from LiveArea'
    assert re.search(r'\b(?:running\s*[=:]\s*(?:false|0)|not running|not_running|stopped|closed)\b',raw,re.I), 'ALICELG01 still running or state unproven'
    assert not re.search(r'\brunning\s*[=:]\s*(?:true|1)\b',raw,re.I), 'ALICELG01 still running'
    return raw

def failure_restore():
    return any(p.ROOT.rglob('STOP.json')) or any(p.ROOT.rglob('ABORT.json')) or (p.ROOT/'RESTORATION-STARTED.json').exists()

def require_closed(reason,final=False):
    p.owner()
    assert (p.ROOT/'TITLE-CLOSED.json').exists(),'Title state unproven: record appstate and inspect HOME; ask the human to close ALICELG01 from LiveArea if needed'
    receipt=json.loads((p.ROOT/'TITLE-CLOSED.json').read_text())
    assert receipt['state']=='home' and receipt['inspection'] and receipt['appstate_command']=='appstate ALICELG01'
    image=p.Path(receipt['path']);assert p.sha(image.read_bytes())==receipt['sha256'],'HOME evidence changed'
    closed_response(receipt['appstate'])
    # Recheck immediately before the writes; never infer closure from a Quit log.
    raw=p.cmd('appstate ALICELG01')
    old=p.B;p.B=p.ROOT
    try:
        p.event('restore-closure-checks.jsonl',dict(reason=reason,appstate_command='appstate ALICELG01',appstate=raw,home=receipt))
        closed_response(raw)
        if final and not (p.ROOT/'RESTORATION-STARTED.json').exists():
            p.rec('RESTORATION-STARTED.json',dict(time=p.now(),reason=reason,appstate=raw,home=receipt,never_in_game_quit=True))
    finally:p.B=old

def live_action():
    assert not (p.ROOT/'RESTORATION-STARTED.json').exists(),'Restoration started: never launch, send gameplay inputs, or use in-game Quit'
    (p.ROOT/'TITLE-CLOSED.json').unlink(missing_ok=True)
