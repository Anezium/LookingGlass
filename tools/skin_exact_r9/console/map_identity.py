"""Canonical runtime maps and logical save visits are separate identities."""
import hashlib, json, re

HUMAN_AUTO = 'ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9'

def verify_runtime(text, entry, frozen=True):
    canonical = entry['map']
    assert canonical in ('hedge3', 'pandemonium')
    assert entry['visit_key'] == ('pandemonium$first' if canonical=='pandemonium' else None)
    loaded = re.findall(r'\[lg/startup\] map_load map=([^\s]+)', text)
    assert loaded == [canonical], ('Canonical runtime map mismatch', loaded, canonical)
    if canonical == 'pandemonium':
        restored = re.findall(r'\[lg/startup\] restored scene reused map=([^\s]+) source=continue-or-bench', text)
        assert restored == [canonical], ('Native save restore receipt missing', restored)
    if frozen:
        maps = re.findall(r'bench: camera frozen map=([^\s]+)', text)
        assert maps == [canonical], ('Canonical frozen map mismatch', maps, canonical)
    return dict(map=canonical, visit_key=entry['visit_key'], runtime_receipts_verified=True)

def save_identity(raw, require_human=True):
    data=json.loads(raw)
    payload=data['payload']
    visit=payload['current']
    level=payload['campaign']['levels'][visit]
    canonical=level['map']
    assert visit=='pandemonium$first' and canonical=='pandemonium', (visit,canonical)
    identity=dict(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest(),map=visit,visit_key=visit,canonical_map=canonical)
    if require_human:
        assert identity['bytes']==181900 and identity['sha256']==HUMAN_AUTO, 'Changed or synthetic human autosave'
    return identity

def verify_save_receipt(receipt):
    assert receipt['sha256']==HUMAN_AUTO and receipt['bytes']==181900
    assert receipt['visit_key']==receipt['map']=='pandemonium$first'
    assert receipt['canonical_map']=='pandemonium'
    assert receipt['fresh_session_start_readback'] and receipt['companions_restored_and_verified']
    assert receipt['independent_readbacks']==2

if __name__=='__main__':
    import argparse
    argparse.ArgumentParser(description=__doc__).parse_args()
