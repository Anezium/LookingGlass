"""Two-orientation wall comparator adapted from matched-observers/console/compare-abba.py.

Retains the original quantiles, oriented deltas and +1 ms per-orientation gate; removes
the obsolete observer-cost pair and requires frame-total direct-mode receipts.
"""
from pathlib import Path
import argparse,json,math,sys
def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root',type=Path,required=True)
    p.add_argument('--block',required=True)
    p.add_argument('--protocol',type=Path,default=Path(__file__).resolve().parents[1]/'CONSOLE-PROTOCOL.json')
    p.add_argument('--output',type=Path)
    args=p.parse_args()
    protocol=json.loads(args.protocol.read_text())
    seq=[x for x in protocol['sequence'] if x['block']==args.block]
    assert [x['arm'] for x in seq]==['A','B','B','A']
    assert all(x['gl_observers']=='direct' for x in seq)
    audits=[json.loads((args.root/x['label']/'AUDIT.json').read_text()) for x in seq]
    audio=[json.loads((args.root/x['label']/'STRICT-AUDIO.json').read_text()) for x in seq]
    assert all(x['status']=='PASS' for x in audits+audio)
    assert all(x['fields_ms']['wall_us']['n']==120 for x in audits)
    assert all(math.isfinite(x['fields_ms']['wall_us']['median']) and x['fields_ms']['wall_us']['median']>0 for x in audits)
    assert all(x['gl_observers']=='direct' and x['observer_checks']['status']=='PASS' and x['observer_checks']['expected_mode']=='direct' for x in audits)
    assert all(x['runtime_config_matches_readback'] and x['complete_benchmark'] and x['clean_exit'] for x in audits)
    poses=[json.loads((args.root/x['label']/'ENTRY-STATE.json').read_text()) for x in seq]
    cam=[x['startup_camera_observations'][0] for x in poses]
    keys=['camera_eye','camera_target','camera_up','feet']
    assert all(all(c.get(k) is not None for k in keys) for c in cam)
    assert all(cam[i][k]==cam[j][k] for i,j in [(0,1),(2,3)] for k in keys),'pose differs within an oriented pair'
    if seq[0]['map']=='pandemonium':
        from map_identity import verify_save_receipt
        receipts=[json.loads((args.root/x['label']/'SAVE-IDENTITY.json').read_text()) for x in seq]
        for receipt in receipts:verify_save_receipt(receipt)
        assert all(x['sha256']==protocol['human_autosave_sha256'] and x['bytes']==181900 and x['map']=='pandemonium$first' and x['fresh_session_start_readback'] and x['companions_restored_and_verified'] for x in receipts)
    values=[round(x['fields_ms']['wall_us']['median']*1000) for x in audits]
    delta=[values[1]-values[0],values[2]-values[3]]
    mean=sum(delta)/2
    both_le=all(d<=1000 for d in delta)
    benefit='REPLICATED_BENEFIT' if all(d < -1000 for d in delta) else 'MIXED_OR_UNRESOLVED_BENEFIT' if any(d < -1000 for d in delta) else 'NO_RESOLVED_BENEFIT'
    status='FAIL_WALL_GATE' if mean>1000 else 'PASS_WALL_GATE' if both_le else 'INCONCLUSIVE_REPEAT_REQUIRED'
    result=dict(status=status,benefit=benefit,block=args.block,map=seq[0]['map'],lever=seq[1]['config_arm'],threshold_ms=1.0,arm_medians_us=values,
        arm_wall_stats_ms=[x['fields_ms']['wall_us'] for x in audits],
        oriented_pair_deltas_ms=[v/1000 for v in delta],average_pair_delta_ms=mean/1000,
        oriented_pair_percent=[100*delta[0]/values[0],100*delta[1]/values[3]],
        average_pair_percent=(100*delta[0]/values[0]+100*delta[1]/values[3])/2,
        A_average_arm_median_ms=(values[0]+values[3])/2000,
        B_average_arm_median_ms=(values[1]+values[2])/2000,
        average_arm_median_percent=100*((values[1]+values[2])/(values[0]+values[3])-1),
        matched_startup_pose_within_pairs=True,cameras=cam,gl_observer_hooks_per_arm=0,
        observer_cost_correction=None,measurement='begin-to-begin wall; all other profiler fields unavailable',
        simulation_identity='full world age and NPC state unavailable; pose equality alone is insufficient',
        next_action='Restore the fresh human-restore state and inspect HOME. A repeat or later session requires fresh ownership/time admission.')
    output=args.output or args.root/(args.block+'-ABBA-GATE.json');output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2));sys.exit(0 if status=='PASS_WALL_GATE' else 2)

if __name__=='__main__':
    main()
