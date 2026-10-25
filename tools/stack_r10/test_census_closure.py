"""Cache closure fixtures for schema 3. No device access."""
from pathlib import Path
import importlib.util,sys
HERE=Path(__file__).resolve().parent
census=next(path for path in (HERE/'console/census_report.py', HERE.parent/'console/census_report.py') if path.exists())
spec=importlib.util.spec_from_file_location('census_report', census)
mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
NAMES=['npc_think_movement','player_movement','fixed_physics','movers','objects','audio_prepare','prepare_acoustics','light_selection','world_opaque','world_visibility','world_parameters','world_submit','alpha_prepare','alpha_submit','actor_gather','actor_collect','actor_publication','actor_submit','fx_gather','joint_finish','fx_submit','hud','present','swap','actor_pose','actor_scales_palette','actor_numeric_capture','actor_serial_skin','actor_worker_dispatch','actor_consumers_publication']

def document(cache_on, cache):
    rows=[]
    for n in range(180):
        rows.append(dict(static_traversal_ns=[1]*5,static_traversal_counts=[[1,0,0,0,0,0,0]]*5,accepted_frame=n+1,measured_frame=None if n<60 else n-59,sequence=n+1,hierarchy_errors=0,frame_total_ns=25_000_000,bracket_union_ns=24_000_000,unbracketed_residual_ns=1_000_000,inclusive_ns=[24_000_000]+[0]*29,exclusive_ns=[24_000_000]+[0]*29,calls=[1]+[0]*29,parent_masks=[1<<30]*24+[1<<14]*6,sweep_count=[1,0,0,0,0],sweep_ns=[1,0,0,0,0],acoustics_world_sweep_count=1,acoustics_world_sweep_ns=1,static_queries_total_hit1_hit4_hit16=[[6,0,0,0],[8,0,0,0],[2,0,0,0]],static_cache_hit_miss_eviction=cache,live_table_evictions=0,actors=[],worker_skin_ns=[0,0]))
    return dict(schema=3,audio_static_cache=cache_on,actor_capacity=128,serial_reasons=['mutable','puppet','frozen','electric','ambient','bounds','topology','publication'],static_count_columns=['queries','nodes','hulls_tested','hulls_clipped','planes','legacy_certificates','fallbacks'],map='hedge3',capacity=180,regions=NAMES,query_classes=['room','ambient','effects'],table_entries=4096,table_probes=16,parent_mask_frame_bit=30,owners=['audio','player','npc','camera','other'],rows=rows)

def expect_fail(data, why):
    try:mod.validate(data,'hedge3')
    except AssertionError:return
    raise SystemExit('validator accepted '+why)

def main():
    mod.validate(document(False, [[0,0,0],[0,0,0],[0,0,0]]),'hedge3')
    mod.validate(document(True, [[4,2,0],[5,3,0],[1,1,0]]),'hedge3')
    expect_fail(document(False, [[1,0,0],[0,0,0],[0,0,0]]), 'cache off with a hit')
    expect_fail(document(True, [[5,0,0],[8,0,0],[2,0,0]]), 'cache on with hit+miss != queries')
    print('PASS census closure fixtures: cache off allows queries with zero counters; cache on requires hit+miss==queries')
if __name__=='__main__':
    main()
