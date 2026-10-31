#!/usr/bin/env python3
"""Host ON/OFF oracle for one new runtime flag. Mac only. No device transport.

tools/skin_exact_r9/host_exact.py stays unchanged and still toggles skin_actual_consumers.
This oracle toggles exactly one flag. Each arm writes dict(base, **prerequisites, <flag>=arm)
into private/vita_renderer.json. The environment is rebuilt with no LOOKING_GLASS_* variable,
then only the probe variables below are set. LOOKING_GLASS_TEST_* never reaches the binary.

Each launch gets a new directory. A launch whose presentation inputs differ from its pair is
kept. Nothing is rewritten. The next attempt is another directory, at most --max-attempts
pairs. The retry condition is the presentation input only: the window size macroquad reports
at frame 0, the GL viewports and scale target of the startup load diagnostics and of frame 0,
and the framebuffer size of every frame. The macOS window settles over the first frames and
its frame-0 size varies between launches of one binary; a cinema camera derives its vertical
FOV from that size. Camera, projection, time, counters, uniforms and images are outputs. They
are never a retry condition. Once a pair shares the input, outputs are compared once. An
output mismatch is not retried.

Checks, per matched pair:
- the printed effective config carries the flag and every prerequisite;
- workload counters named by --witness are 0 in OFF; on each --require-positive map they are
  > 0 in ON. A run whose witnesses are 0 on every map is FAIL (vacuous). Other maps may have
  a zero ON witness when that map has no batched work; OFF is still 0;
- actor_skin, state_sha256, selected_lights_bits, joint_alpha_order, blend_world_order,
  npc_decisions, submitted packets apart from gpu_slot, uniform hashes, named frame-0 uniform
  fields, geometry == gpu_geometry, no serial fallback, frame count, one nonempty image per
  frame, the opening PNG, the final PNG and the census serial/worker vertex totals. The
  opening/final PNG, census and semantic checks are mandatory and run independently of the
  intermediate-image comparison; a failed image comparison never skips them;
- Complete per-launch records before any pair or repeat prerequisite selection. The schema is
  derived from the existing producers: sim_maps/probe.rs, viewer.rs, render/vita_renderer.rs
  and phase_census.rs. Frames carry every semantic array and digest, typed packets with the
  named frame-0 uniform slices bound to the uniform hash, exactly one image per requested
  frame in sequence, non-bool integer dimensions/frame numbers/byte counts, lowercase hex
  digests, bytes == width*height*4, a frame-0 view with its nullable camera viewport, every
  opaque-world cull record, and a census with one row per frame whose accepted_frame,
  sequence, measured_frame, actor identity, fallback and schema evidence and warmup alignment
  match that producer. Nullable producer fields (camera viewport, world cluster, texture,
  actor id) and vacuous arrays are accepted; a missing required field or a bool in a numeric
  identity is not. Equal malformed outputs still fail;
- The host framebuffer is not a reproducible cross-run observable on every intermediate
  frame: two launches of the same arm at the same inputs can differ on a few of 180 hashes
  while every semantic record is equal. A measured same-arm repeat is recorded only after
  its source/binary/config/exit, requested identity, complete schema and semantic/view/
  world/census/opening/final evidence match the reference OFF. Such a repeat is a stability
  observation, never a waiver: every unresolved OFF/ON intermediate-image difference leaves
  the image qualification INCOMPLETE and the command exits nonzero. If the repeated same
  arm is stable the difference is a real mismatch and the run FAILS;
- frame 0 also requires SIM_MAPS_VIEW and SIM_MAPS_WORLD. gl_viewport and scale_target have
  to be present and positive. The frame-0 camera, FOV, projection, depth view and time, and
  every opaque-world cull record (group bounds, model, visibility, draw payload and named
  uniform bits, draw order) match between arms;
- every other probe counter matches, including frame 0. Counters with --counter-prefix
  are the witness and are excluded from that equality. The seven elapsed fields listed
  in ELAPSED_COUNTERS are excluded. cpu_ms and spans_ms are timings, not compared.
  render is null under this probe. An existing output directory is refused.

Lifecycle: every launch identity is written to the attempt ledger before the process is
created, including a launch that then fails. A nonzero exit, an exec/permission error, a
bounded timeout, a JSON decode error, a record/schema defect and a printed-config defect all
end in one terminal FAIL receipt with the failed attempt in the ledger; none leaves the
receipt RUNNING behind an earlier map PASS. Programming errors are not caught as acceptance:
they also close the receipt FAIL and keep their traceback.

Exit status: 0 only for an exact PASS, 3 for an INCOMPLETE image qualification (semantic
PASS is recorded separately), and nonzero for any required mismatch. The census stability
window needs more than 60 frames.
"""
from pathlib import Path
import argparse, hashlib, json, math, os, re, struct, subprocess, sys

DATA = '/Users/anezium/vita-work/alice-data/Alice1/bin/base'
DT = {'pandemonium': '0.04184', 'hedge3': '0.06'}
FRAME_KEYS = ['actor_skin', 'state_sha256', 'selected_lights_bits', 'joint_alpha_order', 'blend_world_order', 'npc_decisions']
U32 = 0xFFFFFFFF
U64 = 0xFFFFFFFFFFFFFFFF
FIELD_WIDTHS = {
    'model': 16, 'projection': 16, 'atlas_grid': 2, 'fullbright': 1, 'lit': 1,
    'clamp_uv': 1, 'cutout': 1, 'cutout_reference': 1, 'gain': 4, 'fog_mode': 1,
    'sky_size': 2, 'sky_fade': 1, 'fx_model': 1, 'fx_appearance': 4, 'lights': 64,
    'camera': 3, 'atmosphere_distance': 4, 'atmosphere_liquid': 4, 'volumes': 20,
}
# Presentation inputs: the window and GL state the frame is drawn into.
VIEW_INPUTS = {'requested_window': 2, 'screen_bits': 2, 'gl_viewport': 4, 'scale_target': 2}
# Values derived from those inputs and from the game state. Compared, never selected on.
VIEW_OUTPUTS = {
    'projection_bits': 16, 'eye_bits': 3, 'target_bits': 3, 'up_bits': 3, 'depth_view_bits': 6,
}
VIEW_OUTPUT_INTS = ('camera_aspect_bits', 'fovy_bits', 'near_bits', 'far_bits', 'time_bits')
WORLD_FIELD_WIDTHS = {
    'mvp': 16, 'model': 16, 'uv': 16, 'uv_after': 16, 'eye': 4, 'surface': 4, 'gain': 4,
    'alpha': 4, 'dot_alpha': 4, 'vector_s': 4, 'vector_t': 4, 'turb': 4, 'deform': 4,
    'wave': 4, 'move_dir': 4, 'atlas': 4, 'fog_eye': 3, 'distance_fog': 4, 'liquid_fog': 4,
    'fog_a_min': 3, 'fog_a_max': 3, 'fog_a': 4, 'fog_b_min': 3, 'fog_b_max': 3, 'fog_b': 4,
}
# Exact record keys emitted by the producers named in the module docstring.
FRAME_RECORD_KEYS = frozenset({
    'actor_skin', 'active', 'blend_world_order', 'counters', 'cpu_ms', 'frame', 'joint_alpha_order',
    'map', 'npc_decisions', 'render', 'selected_lights_bits', 'spans_ms', 'state_sha256',
    'submitted_packets'})
PACKET_RECORD_KEYS = frozenset({
    'geometry', 'gpu_geometry', 'gpu_slot', 'indices', 'policy', 'texture', 'uniforms', 'vertices'})
SKIN_RECORD_KEYS = frozenset({'actor', 'sha256', 'surface'})
NPC_RECORD_KEYS = frozenset({'actor', 'observations', 'sha256'})
IMAGE_RECORD_KEYS = frozenset({'bytes', 'frame', 'height', 'sha256', 'width'})
VIEW_RECORD_KEYS = frozenset({
    'frame', 'requested_window', 'screen_bits', 'camera_aspect_bits', 'fovy_bits', 'eye_bits',
    'target_bits', 'up_bits', 'near_bits', 'far_bits', 'projection_bits', 'depth_view_bits',
    'time_bits', 'camera_viewport', 'gl_viewport', 'scale_target'})
WORLD_RECORD_KEYS = frozenset({
    'frame', 'cluster', 'projection_bits', 'eye_bits', 'time_bits', 'depth_view_bits',
    'fast_frustum', 'draw_order', 'tested'})
WORLD_GROUP_KEYS = frozenset({
    'id', 'visible', 'indices', 'base_element', 'triangles', 'min_bits', 'max_bits',
    'center_bits', 'model_bits', 'cull_model_bits', 'frustum_matrix_bits', 'in_frustum',
    'drawn_indices', 'pipeline', 'low', 'uniform_fields'})
CENSUS_RECORD_KEYS = frozenset({
    'schema', 'actor_capacity', 'serial_reasons', 'audio_static_cache', 'map', 'regions',
    'owners', 'static_count_columns', 'query_classes', 'rows', 'capacity', 'table_entries',
    'table_probes', 'serial_fallbacks', 'parent_mask_frame_bit', 'clock', 'residual',
    'query_rule', 'observer_threshold_ms'})
CENSUS_ROW_KEYS = frozenset({
    'accepted_frame', 'sequence', 'measured_frame', 'frame_total_ns', 'bracket_union_ns',
    'unbracketed_residual_ns', 'inclusive_ns', 'exclusive_ns', 'calls', 'parent_masks',
    'static_traversal_ns', 'static_traversal_counts', 'sweep_count', 'sweep_ns',
    'acoustics_world_sweep_count', 'acoustics_world_sweep_ns',
    'static_queries_total_hit1_hit4_hit16', 'static_cache_hit_miss_eviction',
    'live_table_evictions', 'hierarchy_errors', 'actors', 'worker_skin_ns', 'serial_fallbacks'})
CENSUS_ACTOR_KEYS = frozenset({
    'id', 'serial_vertices', 'worker_vertices', 'serial_reasons', 'reason_vertices'})
# phase_census.rs constants.
CENSUS_REGIONS = (
    'npc_think_movement', 'player_movement', 'fixed_physics', 'movers', 'objects',
    'audio_prepare', 'prepare_acoustics', 'light_selection', 'world_opaque', 'world_visibility',
    'world_parameters', 'world_submit', 'alpha_prepare', 'alpha_submit', 'actor_gather',
    'actor_collect', 'actor_publication', 'actor_submit', 'fx_gather', 'joint_finish',
    'fx_submit', 'hud', 'present', 'swap', 'actor_pose', 'actor_scales_palette',
    'actor_numeric_capture', 'actor_serial_skin', 'actor_worker_dispatch',
    'actor_consumers_publication')
CENSUS_OWNERS = ('audio', 'player', 'npc', 'camera', 'other')
CENSUS_SERIAL_REASONS = (
    'mutable', 'puppet_capability', 'frozen', 'electric', 'ambient_refresh',
    'arena_or_identity', 'topology', 'publication_fallback')
CENSUS_STATIC_COLUMNS = (
    'queries', 'nodes', 'hulls_tested', 'hulls_clipped', 'planes', 'legacy_certificates',
    'fallbacks')
CENSUS_QUERY_CLASSES = ('room', 'ambient', 'effects')
LOAD_STAGE = re.compile(r'^\[lg/load-diagnostic\] reason=(\S+) stage=(\S+) framebuffer_binding=(-?\d+) '
                        r'viewport=\[(-?\d+), (-?\d+), (-?\d+), (-?\d+)\] .* scale_target=(None|Some\(\[(\d+), (\d+)\]\)) ')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canon(value):
    return sha(json.dumps(value, sort_keys=True, separators=(',', ':')).encode())


def packets(frame):
    return [{k: v for k, v in p.items() if k != 'gpu_slot'} for p in frame['submitted_packets']]


def packet_body(packet):
    return {k: v for k, v in packet.items() if k not in ('uniforms', 'uniform_fields')}


# Elapsed samples. Producer definitions: overlap.rs and skin_worker.rs for the
# skin/fx waits, retained.rs for the two upload clocks. Any other name stays gated.
ELAPSED_COUNTERS = frozenset({
    'fx_worker_wait_us',
    'fx_worker_max_service_us',
    'skin_collect_wait_us',
    'skin_gather_us',
    'skin_worker_service_us',
    'vita_fx_upload_gl_us',
    'vita_fx_upload_cpu_us',
})


# A full exact gate either passes, fails a required equality, or leaves the
# intermediate-image qualification incomplete. Incomplete is never success.
INCOMPLETE_EXIT = 3
CENSUS_WARMUP_FRAMES = 60
HEX64 = re.compile(r'^[0-9a-f]{64}$')
LAUNCH_TIMEOUT = 1800.0


class ImageMismatch(AssertionError):
    """Intermediate framebuffer hashes differ; the image gate is not exact."""

    def __init__(self, frames):
        self.frames = list(frames)
        super().__init__(('intermediate pixels', self.frames[:16]))


class ProbeFailure(Exception):
    """One expected operational or validation failure; the caller closes the receipt FAIL."""

    def __init__(self, reason, **detail):
        self.reason = reason
        self.detail = detail
        super().__init__(reason, detail)


def is_int(value):
    return type(value) is int


def u32_value(value):
    return is_int(value) and 0 <= value <= U32


def u64_value(value):
    return is_int(value) and 0 <= value <= U64


def number(value):
    return type(value) in (int, float) and math.isfinite(value)


def hex64(value, label):
    assert isinstance(value, str) and HEX64.fullmatch(value), (label, value)
    return value


def exact_keys(record, keys, label):
    assert isinstance(record, dict), (label, 'record')
    missing = sorted(keys - set(record))
    extra = sorted(set(record) - keys)
    assert not missing and not extra, (label, 'schema', missing, extra)


def u32_list(value, n, label):
    assert isinstance(value, list) and len(value) == n, (label, 'len', None if not isinstance(value, list) else len(value))
    for item in value:
        assert u32_value(item), (label, item)


def u64_list(value, n, label):
    assert isinstance(value, list) and len(value) == n, (label, 'len', None if not isinstance(value, list) else len(value))
    for item in value:
        assert u64_value(item), (label, item)


def u32_matrix(value, rows, cols, label):
    assert isinstance(value, list) and len(value) == rows, (label, 'rows', None if not isinstance(value, list) else len(value))
    for row in value:
        u32_list(row, cols, label)


def u64_matrix(value, rows, cols, label):
    assert isinstance(value, list) and len(value) == rows, (label, 'rows', None if not isinstance(value, list) else len(value))
    for row in value:
        u64_list(row, cols, label)


def workload_counters(frame, counter_prefix):
    return {k: v for k, v in frame['counters'].items()
            if not k.startswith(counter_prefix) and k not in ELAPSED_COUNTERS}


def assert_uniform_fields(fields, label):
    exact_keys(fields, frozenset(FIELD_WIDTHS), (label, 'uniform schema'))
    for key, n in FIELD_WIDTHS.items():
        u32_list(fields[key], n, (label, key))


def assert_uniform_binding(packet, label):
    """The named frame-0 slices must be the bits that the packet uniform hash covers."""
    bits = [value for key in FIELD_WIDTHS for value in packet['uniform_fields'][key]]
    assert len(bits) == 147, (label, 'uniform slice width', len(bits))
    assert sha(struct.pack('<147I', *bits)) == packet['uniforms'], (label, 'named uniform hash')


def assert_packet_schema(packet, frame_index, label):
    keys = PACKET_RECORD_KEYS | ({'uniform_fields'} if frame_index == 0 else set())
    exact_keys(packet, keys, (label, 'packet'))
    hex64(packet['geometry'], (label, 'geometry'))
    hex64(packet['gpu_geometry'], (label, 'gpu_geometry'))
    hex64(packet['uniforms'], (label, 'uniforms'))
    for key in ('gpu_slot', 'vertices', 'indices'):
        assert is_int(packet[key]) and packet[key] >= 0, (label, key, packet[key])
    assert isinstance(packet['policy'], list) and all(u32_value(v) for v in packet['policy']), (
        label, 'policy', packet['policy'][:8])
    assert packet['texture'] is None or hex64(packet['texture'], (label, 'texture')) is not None, (
        label, 'texture')
    if frame_index == 0:
        assert_uniform_fields(packet['uniform_fields'], label)
        assert_uniform_binding(packet, label)


def assert_view(view):
    exact_keys(view, VIEW_RECORD_KEYS, 'frame0 view')
    assert is_int(view['frame']) and view['frame'] == 0, ('view frame', view['frame'])
    for key, n in {**VIEW_INPUTS, **VIEW_OUTPUTS}.items():
        u32_list(view.get(key), n, key)
    for key in VIEW_OUTPUT_INTS:
        value = view.get(key)
        assert u32_value(value), (key, value)
    viewport = view.get('camera_viewport')
    assert viewport is None or (isinstance(viewport, list) and len(viewport) == 2
                                and all(is_int(v) and v >= 0 for v in viewport)), (
        'camera_viewport', viewport)
    assert view['requested_window'] == [960, 544], view['requested_window']
    assert view['gl_viewport'][2] > 0 and view['gl_viewport'][3] > 0, view['gl_viewport']
    assert view['scale_target'][0] > 0 and view['scale_target'][1] > 0, view['scale_target']


def view_outputs(view):
    return {key: view[key] for key in (*VIEW_OUTPUTS, *VIEW_OUTPUT_INTS)}


def parse_load_stages(lines):
    stages = []
    for line in lines:
        m = LOAD_STAGE.match(line)
        if m:
            target = None if m.group(8) == 'None' else [int(m.group(9)), int(m.group(10))]
            stages.append(dict(reason=m.group(1), stage=m.group(2), framebuffer=int(m.group(3)),
                               viewport=[int(m.group(i)) for i in range(4, 8)], scale_target=target))
    return stages


def renderer_prerequisite(view, load_stages, images):
    """Presentation inputs that select a pair. No output is an input here."""
    assert_view(view)
    assert load_stages, 'missing load-diagnostic viewports'
    sizes = [[image.get('width'), image.get('height')] for image in images]
    assert sizes and all(type(w) is int and type(h) is int and w > 0 and h > 0 for w, h in sizes), 'frame sizes'
    return dict(view={key: view[key] for key in ('camera_viewport', *VIEW_INPUTS)},
                load_stages=load_stages, frame_sizes=sizes)


def prerequisite_equal(left, right):
    return left == right


def prerequisite_diff(left, right):
    """Where two presentation inputs differ, for the attempt ledger."""
    out = {key: [left['view'][key], right['view'][key]] for key in left['view'] if left['view'][key] != right['view'][key]}
    if left['load_stages'] != right['load_stages']:
        out['load_stages'] = [left['load_stages'], right['load_stages']]
    sizes = [i for i, (a, b) in enumerate(zip(left['frame_sizes'], right['frame_sizes'])) if a != b]
    if sizes or len(left['frame_sizes']) != len(right['frame_sizes']):
        out['frame_sizes'] = dict(frames=sizes[:16], first=[left['frame_sizes'][sizes[0]], right['frame_sizes'][sizes[0]]] if sizes else None)
    return out


def assert_world_group(group, label):
    exact_keys(group, WORLD_GROUP_KEYS, label)
    for key in ('id', 'indices', 'base_element', 'triangles'):
        assert is_int(group[key]) and group[key] >= 0, (label, key, group[key])
    assert type(group['visible']) is bool, (label, 'visible')
    for key in ('min_bits', 'max_bits', 'center_bits'):
        u32_list(group[key], 3, (label, key))
    for key in ('model_bits', 'cull_model_bits', 'frustum_matrix_bits'):
        if group[key] is not None:
            u32_list(group[key], 16, (label, key))
    assert group['in_frustum'] is None or type(group['in_frustum']) is bool, (label, 'in_frustum')
    assert (group['in_frustum'] is None) == (group['cull_model_bits'] is None), (label, 'cull decision without input')
    assert (group['in_frustum'] is None) == (group['frustum_matrix_bits'] is None), (label, 'frustum matrix without input')
    if group['visible']:
        assert group['in_frustum'] is True and group['model_bits'] is not None, (label, 'drawn outside the recorded frustum')
        assert is_int(group['drawn_indices']) and group['drawn_indices'] > 0, (label, 'drawn payload')
        assert is_int(group['pipeline']) and type(group['low']) is bool, (label, 'drawn pipeline')
        exact_keys(group['uniform_fields'], frozenset({*WORLD_FIELD_WIDTHS, 'all'}), (label, 'world uniform schema'))
        for key, n in WORLD_FIELD_WIDTHS.items():
            u32_list(group['uniform_fields'][key], n, (label, key))
        u32_list(group['uniform_fields']['all'], 143, (label, 'all'))
        assert [bit for key in WORLD_FIELD_WIDTHS for bit in group['uniform_fields'][key]] == group['uniform_fields']['all'], (label, 'world uniform order')
    else:
        assert group['model_bits'] is None, (label, 'model for a culled group')
        assert all(group[key] is None for key in ('drawn_indices', 'pipeline', 'low', 'uniform_fields')), (label, 'culled payload')


def assert_world_records(records, label):
    assert isinstance(records, list) and records, (label, 'missing world records')
    for record in records:
        exact_keys(record, WORLD_RECORD_KEYS, (label, 'world record'))
        assert is_int(record['frame']) and record['frame'] == 0, (label, 'world frame', record['frame'])
        assert record['cluster'] is None or (is_int(record['cluster']) and record['cluster'] >= 0), (
            label, 'cluster', record['cluster'])
        u32_list(record['projection_bits'], 16, (label, 'projection'))
        u32_list(record['eye_bits'], 3, (label, 'eye'))
        u32_list(record['depth_view_bits'], 6, (label, 'depth_view'))
        assert type(record['fast_frustum']) is bool, (label, 'fast_frustum')
        assert u32_value(record['time_bits']), (label, 'time_bits', record['time_bits'])
        assert isinstance(record['tested'], list) and record['tested'], (label, 'no tested groups')
        for group in record['tested']:
            assert_world_group(group, label)
        order = record['draw_order']
        assert isinstance(order, list) and all(is_int(i) and i >= 0 for i in order), (label, 'draw order')
        assert sorted(order) == sorted(g['id'] for g in record['tested'] if g['visible']), (label, 'draw order ids')


def world_mismatch(left, right):
    if left == right:
        return None
    if len(left) != len(right):
        return dict(records=[len(left), len(right)], changed=[], truncated=False)
    def ids(records):
        return [group.get('id') for record in records for group in record.get('tested', [])]
    left_ids, right_ids = ids(left), ids(right)
    changed = []
    for index, (a, b) in enumerate(zip(left, right)):
        if a == b:
            continue
        by_id_a = {group.get('id'): group for group in a.get('tested', [])}
        by_id_b = {group.get('id'): group for group in b.get('tested', [])}
        for gid in sorted(set(by_id_a) | set(by_id_b), key=lambda item: (item is None, item)):
            if by_id_a.get(gid) != by_id_b.get(gid):
                ga, gb = by_id_a.get(gid), by_id_b.get(gid)
                fa = (ga or {}).get('uniform_fields') or {}
                fb = (gb or {}).get('uniform_fields') or {}
                changed.append(dict(record=index, id=gid,
                                     visible=[None if ga is None else ga.get('visible'), None if gb is None else gb.get('visible')],
                                     triangles=[None if ga is None else ga.get('triangles'), None if gb is None else gb.get('triangles')],
                                     drawn_indices=[None if ga is None else ga.get('drawn_indices'), None if gb is None else gb.get('drawn_indices')],
                                     uniform_fields=sorted(k for k in set(fa) | set(fb) if fa.get(k) != fb.get(k))))
                if len(changed) >= 8:
                    return dict(ids=[len(left_ids), len(right_ids)], changed=changed, truncated=True)
    other = sorted({k for a, b in zip(left, right) for k in set(a) | set(b) if k != 'tested' and a.get(k) != b.get(k)})
    return dict(ids=[len(left_ids), len(right_ids)], changed=changed, record_fields=other, truncated=False)


def assert_frame_pair(map_name, index, off_frame, on_frame, counter_prefix):
    for key in FRAME_KEYS:
        assert off_frame[key] == on_frame[key], (map_name, index, key)
    assert off_frame.get('render') is None and on_frame.get('render') is None, (map_name, index, 'render')
    off_packets, on_packets = packets(off_frame), packets(on_frame)
    assert [packet_body(p) for p in off_packets] == [packet_body(p) for p in on_packets], (map_name, index, 'uploaded GPU packets')
    assert [p.get('uniforms') for p in off_packets] == [p.get('uniforms') for p in on_packets], (map_name, index, 'uniform hashes')
    if index == 0:
        for arm_name, arm_packets in (('off', off_packets), ('on', on_packets)):
            for packet_index, packet in enumerate(arm_packets):
                assert_uniform_fields(packet.get('uniform_fields'), (map_name, arm_name, packet_index))
    else:
        assert all('uniform_fields' not in packet for packet in off_packets + on_packets), (map_name, index, 'unexpected uniform fields')
    assert [p.get('uniform_fields') for p in off_packets] == [p.get('uniform_fields') for p in on_packets], (map_name, index, 'uniform fields')
    off_counts, on_counts = workload_counters(off_frame, counter_prefix), workload_counters(on_frame, counter_prefix)
    assert off_counts == on_counts, (map_name, index, 'workload counters', off_counts, on_counts)


def assert_frame_schema(map_name, frames, frame_count, label):
    """Complete producer frame records: identity, semantic arrays, typed packets and digests."""
    assert isinstance(frames, list) and len(frames) == frame_count, (
        label, 'frame count', None if not isinstance(frames, list) else len(frames), frame_count)
    for index, frame in enumerate(frames):
        exact_keys(frame, FRAME_RECORD_KEYS, (label, 'frame record', index))
        assert is_int(frame['frame']) and frame['frame'] == index, (
            label, 'frame index', index, frame['frame'])
        assert isinstance(frame['map'], str) and frame['map'] == map_name, (
            label, 'requested map', index, frame['map'])
        assert frame['active'] is True, (label, 'active identity', index, frame['active'])
        hex64(frame['state_sha256'], (label, 'state_sha256', index))
        assert number(frame['cpu_ms']) and frame['cpu_ms'] >= 0, (label, 'cpu_ms', index)
        assert frame['render'] is None, (label, 'render', index)
        assert isinstance(frame['counters'], dict) and all(
            isinstance(k, str) and u64_value(v) for k, v in frame['counters'].items()), (
            label, 'counters', index)
        assert isinstance(frame['spans_ms'], dict) and all(
            isinstance(k, str) and number(v) for k, v in frame['spans_ms'].items()), (
            label, 'spans_ms', index)
        skins = frame['actor_skin']
        assert isinstance(skins, list), (label, 'actor_skin', index)
        for entry in skins:
            exact_keys(entry, SKIN_RECORD_KEYS, (label, 'actor_skin', index))
            assert entry['actor'] is None or (is_int(entry['actor']) and entry['actor'] >= 0), (
                label, 'actor id', index, entry['actor'])
            assert is_int(entry['surface']) and entry['surface'] >= 0, (label, 'skin surface', index)
            hex64(entry['sha256'], (label, 'skin sha256', index))
        lights = frame['selected_lights_bits']
        assert isinstance(lights, list), (label, 'selected_lights_bits', index)
        for light in lights:
            u32_list(light, 9, (label, 'selected light', index))
        joints = frame['joint_alpha_order']
        assert isinstance(joints, list), (label, 'joint_alpha_order', index)
        for joint in joints:
            assert isinstance(joint, list) and len(joint) == 3, (label, 'joint order', index, joint)
            assert is_int(joint[0]) and 0 <= joint[0] <= 255, (label, 'joint kind', index)
            assert is_int(joint[1]) and joint[1] >= 0, (label, 'joint id', index)
            assert u32_value(joint[2]), (label, 'joint depth bits', index)
        blends = frame['blend_world_order']
        assert isinstance(blends, list), (label, 'blend_world_order', index)
        for blend in blends:
            assert isinstance(blend, list) and len(blend) == 2, (label, 'blend order', index, blend)
            assert is_int(blend[0]) and blend[0] >= 0, (label, 'blend id', index)
            assert type(blend[1]) is bool, (label, 'blend alpha', index)
        npc = frame['npc_decisions']
        assert isinstance(npc, list), (label, 'npc_decisions', index)
        for decision in npc:
            exact_keys(decision, NPC_RECORD_KEYS, (label, 'npc decision', index))
            assert isinstance(decision['actor'], str), (label, 'npc actor', index)
            assert u64_value(decision['observations']), (label, 'npc observations', index)
            hex64(decision['sha256'], (label, 'npc sha256', index))
        assert isinstance(frame['submitted_packets'], list), (label, 'submitted_packets', index)
        for packet_index, packet in enumerate(frame['submitted_packets']):
            assert_packet_schema(packet, index, (label, index, packet_index))


def assert_image_schema(map_name, images, frame_count, label):
    """Complete image records: count, sequence, integer RGBA sizes and hex digests."""
    assert isinstance(images, list) and len(images) == frame_count, (
        map_name, label, 'image count', None if not isinstance(images, list) else len(images), frame_count)
    for index, image in enumerate(images):
        exact_keys(image, IMAGE_RECORD_KEYS, (map_name, label, 'image record', index))
        assert is_int(image['frame']) and image['frame'] == index, (
            map_name, label, 'image index', index, image['frame'])
        width, height = image['width'], image['height']
        assert is_int(width) and width > 0, (map_name, label, 'image width', index, width)
        assert is_int(height) and height > 0, (map_name, label, 'image height', index, height)
        assert is_int(image['bytes']) and image['bytes'] == width * height * 4, (
            map_name, label, 'image RGBA bytes', index, image['bytes'], width * height * 4)
        hex64(image['sha256'], (map_name, label, 'image sha256', index))


def assert_census(map_name, census, frame_count, label):
    """Complete phase_census.rs record: schema, chronology, identity, fallback and warmup."""
    exact_keys(census, CENSUS_RECORD_KEYS, (map_name, label, 'census record'))
    assert is_int(census['schema']) and census['schema'] == 3, (map_name, label, 'census schema', census['schema'])
    assert census['map'] == map_name, (map_name, label, 'census map', census['map'])
    assert is_int(census['actor_capacity']) and census['actor_capacity'] == 128, (
        map_name, label, 'actor capacity', census['actor_capacity'])
    assert census['regions'] == list(CENSUS_REGIONS), (map_name, label, 'census regions')
    assert census['owners'] == list(CENSUS_OWNERS), (map_name, label, 'census owners')
    assert census['serial_reasons'] == list(CENSUS_SERIAL_REASONS), (map_name, label, 'serial reasons')
    assert census['static_count_columns'] == list(CENSUS_STATIC_COLUMNS), (map_name, label, 'static columns')
    assert census['query_classes'] == list(CENSUS_QUERY_CLASSES), (map_name, label, 'query classes')
    assert type(census['audio_static_cache']) is bool, (map_name, label, 'audio static cache')
    for key in ('clock', 'residual', 'query_rule'):
        assert isinstance(census[key], str) and census[key], (map_name, label, 'census text', key)
    assert number(census['observer_threshold_ms']) and census['observer_threshold_ms'] == 1.0, (
        map_name, label, 'observer threshold', census['observer_threshold_ms'])
    rows = census['rows']
    assert isinstance(rows, list) and len(rows) == frame_count, (
        map_name, label, 'census row count', None if not isinstance(rows, list) else len(rows), frame_count)
    assert frame_count > CENSUS_WARMUP_FRAMES, (map_name, label, 'census warmup window', frame_count)
    assert is_int(census['capacity']) and census['capacity'] == frame_count, (
        map_name, label, 'census capacity', census['capacity'])
    assert is_int(census['table_entries']) and census['table_entries'] == 4096, (
        map_name, label, 'table entries', census['table_entries'])
    assert is_int(census['table_probes']) and census['table_probes'] == 16, (
        map_name, label, 'table probes', census['table_probes'])
    assert is_int(census['parent_mask_frame_bit']) and census['parent_mask_frame_bit'] == len(CENSUS_REGIONS), (
        map_name, label, 'parent mask frame bit', census['parent_mask_frame_bit'])
    assert u64_value(census['serial_fallbacks']) and census['serial_fallbacks'] == 0, (
        map_name, label, 'serial fallbacks', census['serial_fallbacks'])
    previous = None
    for index, row in enumerate(rows):
        exact_keys(row, CENSUS_ROW_KEYS, (map_name, label, 'census row', index))
        assert is_int(row['accepted_frame']) and row['accepted_frame'] == index + 1, (
            map_name, label, 'accepted_frame', index, row['accepted_frame'])
        assert u64_value(row['sequence']) and row['sequence'] >= index + 1, (
            map_name, label, 'sequence', index, row['sequence'])
        assert previous is None or row['sequence'] > previous, (
            map_name, label, 'sequence order', index, row['sequence'], previous)
        previous = row['sequence']
        measured = row['measured_frame']
        if index < CENSUS_WARMUP_FRAMES:
            assert measured is None, (map_name, label, 'warmup row measured', index, measured)
        else:
            assert is_int(measured) and measured == index - CENSUS_WARMUP_FRAMES + 1, (
                map_name, label, 'measured_frame', index, measured)
        total, coverage = row['frame_total_ns'], row['bracket_union_ns']
        assert u64_value(total) and total > 0, (map_name, label, 'frame total', index, total)
        assert u64_value(coverage) and coverage <= total, (map_name, label, 'bracket union', index, coverage)
        assert u64_value(row['unbracketed_residual_ns']) and row['unbracketed_residual_ns'] == total - coverage, (
            map_name, label, 'unbracketed residual', index)
        u64_list(row['inclusive_ns'], len(CENSUS_REGIONS), (map_name, label, 'inclusive', index))
        u64_list(row['exclusive_ns'], len(CENSUS_REGIONS), (map_name, label, 'exclusive', index))
        u32_list(row['calls'], len(CENSUS_REGIONS), (map_name, label, 'calls', index))
        u32_list(row['parent_masks'], len(CENSUS_REGIONS), (map_name, label, 'parent masks', index))
        u64_list(row['static_traversal_ns'], 5, (map_name, label, 'static traversal', index))
        u64_matrix(row['static_traversal_counts'], 5, 7, (map_name, label, 'static counts', index))
        u32_list(row['sweep_count'], 5, (map_name, label, 'sweep count', index))
        u64_list(row['sweep_ns'], 5, (map_name, label, 'sweep ns', index))
        assert u32_value(row['acoustics_world_sweep_count']), (map_name, label, 'acoustics count', index)
        assert u64_value(row['acoustics_world_sweep_ns']), (map_name, label, 'acoustics ns', index)
        u32_matrix(row['static_queries_total_hit1_hit4_hit16'], 3, 4, (map_name, label, 'static queries', index))
        u32_matrix(row['static_cache_hit_miss_eviction'], 3, 3, (map_name, label, 'static cache', index))
        assert u32_value(row['live_table_evictions']), (map_name, label, 'evictions', index)
        assert u32_value(row['hierarchy_errors']) and row['hierarchy_errors'] == 0, (
            map_name, label, 'hierarchy errors', index, row['hierarchy_errors'])
        assert isinstance(row['actors'], list), (map_name, label, 'census actors', index)
        for actor in row['actors']:
            exact_keys(actor, CENSUS_ACTOR_KEYS, (map_name, label, 'census actor', index))
            assert u64_value(actor['id']), (map_name, label, 'actor id', index, actor['id'])
            for key in ('serial_vertices', 'worker_vertices'):
                assert u32_value(actor[key]), (map_name, label, 'census vertex count', index, key, actor[key])
            assert u32_value(actor['serial_reasons']), (map_name, label, 'serial reasons', index)
            u32_list(actor['reason_vertices'], len(CENSUS_SERIAL_REASONS), (map_name, label, 'reason vertices', index))
        u64_list(row['worker_skin_ns'], 2, (map_name, label, 'worker skin', index))
        assert u64_value(row['serial_fallbacks']) and row['serial_fallbacks'] == 0, (
            map_name, label, 'row serial fallbacks', index, row['serial_fallbacks'])
    assert all(row['actors'] for row in rows[CENSUS_WARMUP_FRAMES:]), (
        map_name, label, 'populated census rows')


def census_vertices(census):
    rows = census['rows'][CENSUS_WARMUP_FRAMES:]
    return (sum(actor['serial_vertices'] for row in rows for actor in row['actors']),
            sum(actor['worker_vertices'] for row in rows for actor in row['actors']))


def differing_images(left, right):
    return [index for index, (a, b) in enumerate(zip(left, right)) if a != b]


def git_text(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True)


def source_tree(root):
    porcelain = git_text(root, 'status', '--porcelain=v1', '--untracked-files=all')
    lines = [line for line in porcelain.splitlines() if line.strip()]
    assert lines in ([], [' M docs/RCHESS1.md'], ['M docs/RCHESS1.md']), lines
    return dict(
        head=git_text(root, 'rev-parse', 'HEAD').strip(),
        tree=git_text(root, 'rev-parse', 'HEAD^{tree}').strip(),
        porcelain=porcelain,
    )


def parse_view(lines):
    views = []
    for line in lines:
        if line.startswith('SIM_MAPS_VIEW '):
            views.append(json.loads(line.split(' ', 1)[1]))
    return views


def parse_world(lines):
    worlds = []
    for line in lines:
        if line.startswith('SIM_MAPS_WORLD '):
            worlds.append(json.loads(line.split(' ', 1)[1]))
    return worlds


def decode_json(text, source):
    try:
        return json.loads(text)
    except ValueError as exc:
        raise ProbeFailure('decode error', classification='decode-error', source=source,
                           error=f'{type(exc).__name__}: {exc}', text=text[:160]) from exc


def decode_record(line, prefix):
    return decode_json(line[len(prefix):], prefix)


def main():
    p = argparse.ArgumentParser()
    p.add_argument('output', type=Path)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--flag', required=True)
    p.add_argument('--maps', nargs='+', required=True)
    p.add_argument('--prerequisites', required=True, help='JSON object written into both arms')
    p.add_argument('--witness', action='append', required=True, help='counter 0 in OFF; > 0 in ON on required maps')
    p.add_argument('--require-positive', action='append', required=True, help='map whose ON witness must be > 0')
    p.add_argument('--counter-prefix', required=True)
    p.add_argument('--frames', type=int, default=180)
    p.add_argument('--label', default='host-flag-exact')
    p.add_argument('--max-attempts', type=int, default=8)
    p.add_argument('--repeat-off', action='append', default=[], help='map that also needs a second OFF launch with the same prerequisite')
    p.add_argument('--launch-timeout', type=float, default=LAUNCH_TIMEOUT,
                   help='bounded seconds before one launch is a terminal operational failure')
    args = p.parse_args()
    assert 1 <= args.max_attempts <= 8, args.max_attempts
    assert 0 < args.launch_timeout <= LAUNCH_TIMEOUT, args.launch_timeout
    assert args.frames > CENSUS_WARMUP_FRAMES, (
        'the census stability window needs more than', CENSUS_WARMUP_FRAMES, 'frames')
    root = Path(__file__).resolve().parents[2]
    out = args.output.resolve()
    if out.exists() and any(out.iterdir()):
        raise SystemExit(f'refuse reuse of {out}')
    out.mkdir(parents=True)
    base = json.loads((root / 'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
    prerequisites = json.loads(args.prerequisites)
    assert isinstance(prerequisites, dict) and prerequisites, prerequisites
    assert args.flag not in prerequisites and args.flag not in base, 'the flag is supplied only by the arm'
    required = set(args.require_positive)
    assert required <= set(args.maps), (sorted(required - set(args.maps)))
    repeat_off = set(args.repeat_off)
    assert repeat_off <= set(args.maps), sorted(repeat_off - set(args.maps))
    tree = source_tree(root)
    binary_sha = sha(args.binary.read_bytes())
    oracle_sha = sha(Path(__file__).read_bytes())
    receipt_path = out / 'proof' / f'{args.label}.json'
    ledger_path = out / 'proof' / 'attempts.jsonl'
    ledger_path.parent.mkdir(parents=True, exist_ok=True)
    result = dict(
        label=args.label, binary=str(args.binary), binary_sha256=binary_sha,
        flag=args.flag, prerequisites=prerequisites, witness=args.witness,
        require_positive=sorted(required), counter_prefix=args.counter_prefix,
        frames=args.frames, max_attempts=args.max_attempts, repeat_off=sorted(repeat_off),
        launch_timeout=args.launch_timeout,
        maps=[], status='RUNNING', elapsed_counters=sorted(ELAPSED_COUNTERS),
        head=tree['head'], tree=tree['tree'], porcelain=tree['porcelain'],
        oracle_sha256=oracle_sha, ledger='proof/attempts.jsonl')
    positive = {name: 0 for name in args.witness}

    def note(row):
        with ledger_path.open('a') as ledger:
            ledger.write(json.dumps(row, sort_keys=True) + '\n')

    def launch(map_name, arm, attempt):
        d = out / 'host' / args.label / f'{map_name}-{args.flag}-{int(arm)}-a{attempt}'
        (d / 'private').mkdir(parents=True, exist_ok=False)
        config = dict(base, **prerequisites, **{args.flag: arm}, phase_census=True)
        config_text = json.dumps(config)
        (d / 'private/vita_renderer.json').write_text(config_text)
        env = {k: v for k, v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
        probe_env = dict(
            LOOKING_GLASS_SIM_MAPS_PROBE='1',
            LOOKING_GLASS_SIM_MAPS_DT=DT.get(map_name, '0.05'),
            LOOKING_GLASS_SKIN_PROOF='1',
            LOOKING_GLASS_SKIN_MOVING_VIEW='1',
            LOOKING_GLASS_SKIN_DEATHS='1',
            LOOKING_GLASS_SIM_MAPS_RENDER_INPUTS='1',
            LOOKING_GLASS_SIM_MAPS_FRAME_IMAGE='1')
        env.update(probe_env)
        assert 'LOOKING_GLASS_SIM_MAPS_COUNTERS' not in env
        assert not any(k.startswith('LOOKING_GLASS_TEST_') for k in env)
        cmd = [str(args.binary), '--data', DATA, '--no-audio', '--save-dir', str(d / 'private/saves'),
               '--map', map_name, '--frames', str(args.frames), '--capture', str(d / 'final.png')]
        (d / 'argv.json').write_text(json.dumps(dict(argv=cmd, probe_env=probe_env, binary_sha256=binary_sha)) + '\n')
        # The attempted launch identity is durable before the process exists, so a
        # process-start failure still names its map, arm, attempt and output directory.
        identity = dict(map=map_name, arm=arm, attempt=attempt, directory=str(d),
                        argv=cmd, probe_env=probe_env, binary_sha256=binary_sha, head=tree['head'],
                        tree=tree['tree'], oracle_sha256=oracle_sha,
                        config_sha256=sha(config_text.encode()))
        note(dict(identity, classification='launch-start'))
        try:
            with (d / 'run.log').open('w') as handle:
                proc = subprocess.run(cmd, cwd=d, env=env, stdout=handle,
                                      stderr=subprocess.STDOUT, timeout=args.launch_timeout)
        except subprocess.TimeoutExpired as exc:
            note(dict(identity, classification='timeout', timeout=args.launch_timeout))
            raise ProbeFailure('launch timeout', map=map_name, arm=arm, attempt=attempt,
                               directory=str(d), timeout=args.launch_timeout) from exc
        except OSError as exc:
            note(dict(identity, classification='launch-error', error=f'{type(exc).__name__}: {exc}'))
            raise ProbeFailure('launch process error', map=map_name, arm=arm, attempt=attempt,
                               directory=str(d), error=f'{type(exc).__name__}: {exc}') from exc
        row = dict(identity, exit=proc.returncode, log_sha256=sha((d / 'run.log').read_bytes()))
        try:
            current = sha(args.binary.read_bytes())
        except OSError as exc:
            note(dict(row, classification='binary-read-error', error=f'{type(exc).__name__}: {exc}'))
            raise ProbeFailure('binary read error', map=map_name, arm=arm, attempt=attempt,
                               directory=str(d), error=f'{type(exc).__name__}: {exc}') from exc
        if current != binary_sha:
            note(dict(row, classification='binary-changed', current=current))
            raise ProbeFailure('binary changed during launch', map=map_name, arm=arm, attempt=attempt,
                               directory=str(d), current=current, expected=binary_sha)
        if proc.returncode != 0:
            note(dict(row, classification='exit'))
            raise ProbeFailure('arm launch failed', map=map_name, arm=arm, attempt=attempt,
                               directory=str(d), exit=proc.returncode, log=str(d / 'run.log'))
        try:
            text = (d / 'run.log').read_text()
            lines = text.splitlines()
            images = [decode_record(line, 'SKIN_EXACT_IMAGE') for line in lines if line.startswith('SKIN_EXACT_IMAGE ')]
            frames = [decode_record(line, 'SIM_MAPS_FRAME') for line in lines if line.startswith('SIM_MAPS_FRAME ')]
            census_path = d / f'{map_name}-census-{args.frames}.json'
            census = decode_json(census_path.read_text(), str(census_path))
            views = [decode_record(line, 'SIM_MAPS_VIEW') for line in lines if line.startswith('SIM_MAPS_VIEW ')]
            worlds = [decode_record(line, 'SIM_MAPS_WORLD') for line in lines if line.startswith('SIM_MAPS_WORLD ')]
        except OSError as exc:
            note(dict(row, classification='record-read-error', error=f'{type(exc).__name__}: {exc}'))
            raise ProbeFailure('record read error', map=map_name, arm=arm, attempt=attempt,
                               directory=str(d), error=f'{type(exc).__name__}: {exc}') from exc
        except ProbeFailure as exc:
            note(dict(row, classification=exc.detail.get('classification', 'decode-error'),
                      error=exc.detail.get('error', str(exc))))
            raise
        try:
            # Complete launch schema and identity precede any selection or comparison.
            # A malformed record is a probe defect, never a presentation retry.
            assert_frame_schema(map_name, frames, args.frames, (map_name, arm, attempt))
            assert_image_schema(map_name, images, args.frames, (map_name, arm, attempt))
            assert_census(map_name, census, args.frames, (map_name, arm, attempt))
            assert len(views) == 1, (map_name, arm, len(views))
            assert_view(views[0])
            prereq = renderer_prerequisite(views[0], parse_load_stages(lines), images)
            assert_world_records(worlds, (map_name, arm, attempt))
            final_png = d / 'final.png'
            assert final_png.is_file() and final_png.stat().st_size > 0, (map_name, arm, 'final image')
            configs = [decode_record(line, '[lg/vita-renderer] config ') for line in lines
                       if '[lg/vita-renderer] config ' in line]
            assert len(configs) == 1, (map_name, arm, len(configs))
            printed = configs[0]
            assert isinstance(printed, dict), (map_name, arm, 'printed config', 'object')
            assert args.flag in printed, (map_name, arm, 'printed config', 'missing flag', args.flag)
            for key in prerequisites:
                assert key in printed, (map_name, arm, 'printed config', 'missing prerequisite', key)
            assert 'phase_census' in printed, (map_name, arm, 'printed config', 'missing phase_census')
            assert printed[args.flag] is arm, (map_name, arm, 'printed flag', printed.get(args.flag))
            for key, value in prerequisites.items():
                assert printed[key] == value, (map_name, arm, 'prerequisite', key, printed.get(key), value)
            assert printed['phase_census'] is True, (map_name, arm, 'phase_census')
            single = [line for line in lines if line.startswith('[lg/vita-renderer] actor_single_prepare active=')]
            assert single == ['[lg/vita-renderer] actor_single_prepare active=true'], (map_name, arm, single)
            fallback = [line for line in lines if '[lg/skin] serial fallback' in line or 'afterimage consumer must be declared' in line]
            assert not fallback, (map_name, arm, fallback[:3])
            assert all(all(packet['geometry'] == packet['gpu_geometry'] for packet in frame['submitted_packets']) for frame in frames), (map_name, arm, 'gpu readback')
            opening_png = d / 'private/opening-frame.png'
            assert opening_png.is_file() and opening_png.stat().st_size > 0, (map_name, arm, 'opening frame image')
            assert any(image.get('frame') == 0 and image.get('bytes', 0) > 0 for image in images), (map_name, arm, 'opening frame record')
        except AssertionError as exc:
            note(dict(row, classification='invalid-record', error=str(exc)))
            raise ProbeFailure('invalid record', map=map_name, arm=arm, attempt=attempt,
                               directory=str(d), error=str(exc)) from exc
        except ProbeFailure as exc:
            note(dict(row, classification=exc.detail.get('classification', 'decode-error'),
                      error=exc.detail.get('error', str(exc))))
            raise
        except (KeyError, TypeError, ValueError, AttributeError, IndexError) as exc:
            note(dict(row, classification='invalid-record', error=f'{type(exc).__name__}: {exc}'))
            raise ProbeFailure('invalid record', map=map_name, arm=arm, attempt=attempt,
                               directory=str(d), error=f'{type(exc).__name__}: {exc}') from exc
        except OSError as exc:
            note(dict(row, classification='record-read-error', error=f'{type(exc).__name__}: {exc}'))
            raise ProbeFailure('record read error', map=map_name, arm=arm, attempt=attempt,
                               directory=str(d), error=f'{type(exc).__name__}: {exc}') from exc
        serial_vertices, worker_vertices = census_vertices(census)
        row['classification'] = 'prerequisite-present'
        row['prerequisite_sha256'] = canon(prereq)
        row['world_sha256'] = canon(worlds)
        note(row)
        counters = {}
        for frame in frames:
            for key, value in frame['counters'].items():
                counters[key] = counters.get(key, 0) + value
        print('DONE', args.label, map_name, arm, 'attempt', attempt, 'frames', len(frames), flush=True)
        return dict(
            ok=True, arm=arm, attempt=attempt, directory=str(d), frames=frames, images=images,
            worlds=worlds, prerequisite=prereq, view=view_outputs(views[0]), counters=counters,
            serial_vertices=serial_vertices,
            worker_vertices=worker_vertices,
            final_png_sha256=sha(final_png.read_bytes()),
            opening_png_sha256=sha(opening_png.read_bytes()),
            census_sha256=sha(census_path.read_bytes()),
            run_log_sha256=row['log_sha256'],
            head=tree['head'], tree=tree['tree'], binary_sha256=binary_sha,
            oracle_sha256=oracle_sha, config_sha256=row['config_sha256'], exit=proc.returncode,
            printed_flag=printed[args.flag],
            printed_prerequisites={key: printed[key] for key in prerequisites},
            gpu_draw_readbacks=sum(len(frame['submitted_packets']) for frame in frames))

    def compare(map_name, off, on, images_equal=True):
        # Schema is re-checked here so no caller can compare an incomplete arm.
        assert_image_schema(map_name, off['images'], args.frames, (map_name, 'compare-off'))
        assert_image_schema(map_name, on['images'], args.frames, (map_name, 'compare-on'))
        # Every mandatory equality runs before any image verdict is raised. An image
        # difference can therefore never skip a semantic or mandatory output check.
        problems = []
        if not prerequisite_equal(off['prerequisite'], on['prerequisite']):
            problems.append(('prerequisite',))
        if off['view'] != on['view']:
            problems.append(('frame-0 camera', sorted(k for k in off['view'] if off['view'][k] != on['view'][k])))
        mismatch = world_mismatch(off['worlds'], on['worlds'])
        if mismatch is not None:
            problems.append(('world cull', mismatch))
        for index, (left, right) in enumerate(zip(off['frames'], on['frames'])):
            try:
                assert_frame_pair(map_name, index, left, right, args.counter_prefix)
            except AssertionError as exc:
                problems.append(exc.args[0] if exc.args else str(exc))
        differences = differing_images(off['images'], on['images'])
        if off['final_png_sha256'] != on['final_png_sha256']:
            problems.append(('final image',))
        if off['opening_png_sha256'] != on['opening_png_sha256']:
            problems.append(('opening frame image',))
        if (off['serial_vertices'], off['worker_vertices']) != (on['serial_vertices'], on['worker_vertices']):
            problems.append(('census vertices',))
        if problems:
            raise AssertionError((map_name, problems[:8]))
        if images_equal and differences:
            raise ImageMismatch(differences)
        return differences

    def public_arm(arm):
        return dict(
            attempt=arm['attempt'], directory=arm['directory'],
            head=arm['head'], tree=arm['tree'], binary_sha256=arm['binary_sha256'],
            oracle_sha256=arm['oracle_sha256'], config_sha256=arm['config_sha256'], exit=arm['exit'],
            prerequisite_sha256=canon(arm['prerequisite']), view=arm['view'],
            world_sha256=canon(arm['worlds']), opening_png_sha256=arm['opening_png_sha256'],
            final_png_sha256=arm['final_png_sha256'], run_log_sha256=arm['run_log_sha256'],
            census_sha256=arm['census_sha256'],
            gpu_draw_readbacks=arm['gpu_draw_readbacks'],
            printed_flag=arm['printed_flag'], printed_prerequisites=arm['printed_prerequisites'])

    def fail_map(map_name, reason, **detail):
        result['status'] = 'FAIL'
        result['error'] = dict(detail, map=map_name, reason=reason)
        receipt_path.parent.mkdir(parents=True, exist_ok=True)
        receipt_path.write_text(json.dumps(result, indent=1) + '\n')
        note(dict(map=map_name, classification='fail', reason=reason, detail=detail))
        raise AssertionError(result['error'])

    incomplete_maps = []

    def run_maps():
        for map_name in args.maps:
            matched = None
            kept = []
            for attempt in range(args.max_attempts):
                try:
                    off = launch(map_name, False, attempt)
                    on = launch(map_name, True, attempt)
                except ProbeFailure as exc:
                    fail_map(map_name, exc.reason, **exc.detail)
                kept.append(dict(attempt=attempt, off=off['directory'], on=on['directory'],
                                 off_prerequisite_sha256=canon(off['prerequisite']),
                                 on_prerequisite_sha256=canon(on['prerequisite'])))
                if prerequisite_equal(off['prerequisite'], on['prerequisite']):
                    note(dict(map=map_name, attempt=attempt, classification='prerequisite-matched',
                              prerequisite_sha256=canon(off['prerequisite'])))
                    matched = (off, on)
                    break
                note(dict(map=map_name, attempt=attempt, classification='prerequisite-mismatch',
                          off_sha256=canon(off['prerequisite']), on_sha256=canon(on['prerequisite']),
                          differing=prerequisite_diff(off['prerequisite'], on['prerequisite'])))
            if matched is None:
                fail_map(map_name, 'no matched renderer prerequisite', attempts=kept)
            off, on = matched
            repeat = None
            stability = None
            image_status = 'EXACT'
            image_differences = []
            try:
                compare(map_name, off, on)
            except ImageMismatch as exc:
                # An intermediate-image difference is never accepted from repeat noise. A
                # fully validated same-arm repeat only records whether pixels are stable.
                image_differences = list(exc.frames)
                image_status = 'INCOMPLETE'
                for extra in range(args.max_attempts):
                    try:
                        again = launch(map_name, False, args.max_attempts + extra)
                    except ProbeFailure as launch_error:
                        fail_map(map_name, 'image repeat launch failed', error=str(launch_error),
                                 failure=launch_error.detail)
                    if prerequisite_equal(again['prerequisite'], off['prerequisite']):
                        repeat = again
                        break
                    note(dict(map=map_name, attempt=args.max_attempts + extra,
                              classification='image-repeat-mismatch',
                              differing=prerequisite_diff(again['prerequisite'], off['prerequisite'])))
                if repeat is None:
                    note(dict(map=map_name, attempt=off['attempt'], classification='image-stability-unavailable',
                              differing=image_differences[:32]))
                    stability = dict(available=False, differing=image_differences[:32],
                                     note='no matched same-arm launch; image qualification stays incomplete')
                else:
                    try:
                        repeat_differences = compare(map_name, off, repeat, images_equal=False)
                    except AssertionError as exc:
                        note(dict(map=map_name, attempt=repeat['attempt'], classification='repeat-output-fail', error=str(exc)))
                        fail_map(map_name, 'same-arm repeat failed semantic or mandatory evidence', error=str(exc))
                    stability = dict(available=True, directory=repeat['directory'], attempt=repeat['attempt'],
                                     explicit=False, differing=repeat_differences[:32],
                                     same_arm_stable=not repeat_differences, validated_against_off=True,
                                     note='validated repeat evidence; a repeat never waives a candidate image difference')
                    note(dict(map=map_name, attempt=off['attempt'], classification='image-stability-observation',
                              differing=image_differences[:32], same_arm_differing=repeat_differences[:32],
                              same_arm_stable=not repeat_differences))
                    if not repeat_differences:
                        fail_map(map_name, 'image difference with a stable same-arm control',
                                 differing=image_differences[:32])
            except AssertionError as exc:
                note(dict(map=map_name, attempt=off['attempt'], classification='output-fail', error=str(exc)))
                fail_map(map_name, 'output mismatch after matched prerequisite', error=str(exc))
            if map_name in repeat_off:
                if repeat is None:
                    for extra in range(args.max_attempts):
                        try:
                            again = launch(map_name, False, args.max_attempts + extra)
                        except ProbeFailure as launch_error:
                            fail_map(map_name, 'repeated OFF launch failed', error=str(launch_error),
                                     failure=launch_error.detail)
                        if prerequisite_equal(again['prerequisite'], off['prerequisite']):
                            repeat = again
                            break
                        note(dict(map=map_name, attempt=args.max_attempts + extra, classification='repeat-off-mismatch',
                                  differing=prerequisite_diff(again['prerequisite'], off['prerequisite'])))
                    if repeat is None:
                        fail_map(map_name, 'repeated OFF never shared the matched prerequisite')
                try:
                    repeat_differences = compare(map_name, off, repeat, images_equal=False)
                except AssertionError as exc:
                    fail_map(map_name, 'repeated OFF failed semantic or mandatory evidence', error=str(exc))
                if stability is None:
                    stability = dict(available=True, directory=repeat['directory'], attempt=repeat['attempt'],
                                     explicit=True, differing=repeat_differences[:32],
                                     same_arm_stable=not repeat_differences, validated_against_off=True,
                                     note='validated repeated OFF evidence; intermediate image equality is not asserted')
                note(dict(map=map_name, attempt=repeat['attempt'], classification='repeat-off-matched'))
            try:
                witness = {}
                for name in args.witness:
                    pair = [off['counters'].get(name, 0), on['counters'].get(name, 0)]
                    witness[name] = pair
                    assert pair[0] == 0, (map_name, 'OFF witness', name, pair[0])
                    if map_name in required:
                        assert pair[1] > 0, (map_name, 'vacuous ON witness', name, pair[1])
                    positive[name] += pair[1]
            except AssertionError as exc:
                fail_map(map_name, 'witness mismatch', error=str(exc))
            prefixed = {k: [off['counters'].get(k, 0), on['counters'].get(k, 0)]
                        for k in sorted(set(off['counters']) | set(on['counters'])) if k.startswith(args.counter_prefix)}
            row = dict(
                map=map_name, dt=DT.get(map_name, '0.05'), frames=args.frames, witness_off_on=witness,
                prefixed_counters_off_on=prefixed, required_positive=map_name in required,
                attempts=kept, matched_attempt=off['attempt'],
                prerequisite=off['prerequisite'],
                frame_digest=[canon([{k: frame[k] for k in FRAME_KEYS} for frame in arm['frames']]) for arm in (off, on)],
                packet_digest=[canon([packets(frame) for frame in arm['frames']]) for arm in (off, on)],
                image_digest=[canon(arm['images']) for arm in (off, on)], image_witnesses=len(off['images']),
                world_sha256=[canon(off['worlds']), canon(on['worlds'])],
                status='PASS' if image_status == 'EXACT' else 'INCOMPLETE',
                semantic_status='PASS', image_status=image_status,
                image_differences=image_differences[:32], stability_observation=stability,
                final_png_sha256=[off['final_png_sha256'], on['final_png_sha256']],
                opening_png_sha256=[off['opening_png_sha256'], on['opening_png_sha256']],
                census_sha256=[off['census_sha256'], on['census_sha256']],
                elapsed_off_on={name: [off['counters'].get(name, 0), on['counters'].get(name, 0)] for name in sorted(ELAPSED_COUNTERS)},
                run_log_sha256=[off['run_log_sha256'], on['run_log_sha256']],
                gpu_draw_readbacks=[off['gpu_draw_readbacks'], on['gpu_draw_readbacks']],
                census_serial_worker_vertices=[[off['serial_vertices'], off['worker_vertices']], [on['serial_vertices'], on['worker_vertices']]],
                printed=[dict(flag=arm['printed_flag'], prerequisites=arm['printed_prerequisites']) for arm in (off, on)],
                arms=[public_arm(off), public_arm(on)])
            if repeat is not None:
                row['repeat_off'] = public_arm(repeat)
                row['repeat_off']['frame_digest'] = canon([{k: frame[k] for k in FRAME_KEYS} for frame in repeat['frames']])
                row['repeat_off']['packet_digest'] = canon([packets(frame) for frame in repeat['frames']])
                row['repeat_off']['census_vertices'] = [repeat['serial_vertices'], repeat['worker_vertices']]
            result['maps'].append(row)
            if image_status != 'EXACT':
                incomplete_maps.append(map_name)
            receipt_path.write_text(json.dumps(result, indent=1) + '\n')
            if image_status == 'EXACT':
                print('PASS', args.label, map_name, 'witness', json.dumps(witness), flush=True)
            else:
                print('INCOMPLETE', args.label, map_name, 'semantic PASS', 'differing', image_differences[:16],
                      'witness', json.dumps(witness), flush=True)

    try:
        run_maps()
        assert all(v > 0 for v in positive.values()), ('vacuous oracle', positive)
        after = source_tree(root)
        assert after['tree'] == tree['tree'], ('source changed during oracle', tree['tree'], after['tree'])
        assert sha(args.binary.read_bytes()) == binary_sha
        assert sha(Path(__file__).read_bytes()) == oracle_sha
    except Exception as exc:
        # An unexpected failure is never acceptance. Close the receipt FAIL while the
        # original exception and traceback stay visible.
        if result['status'] == 'RUNNING':
            result['status'] = 'FAIL'
            result['error'] = dict(map=None, reason='oracle failure',
                                   error=f'{type(exc).__name__}: {exc}')
            receipt_path.write_text(json.dumps(result, indent=1) + '\n')
            note(dict(map=None, classification='fail', reason='oracle failure',
                      detail={'error': f'{type(exc).__name__}: {exc}'}))
        raise
    result['witness_on_total'] = positive
    result['source_after'] = after
    result['semantic_status'] = 'PASS'
    if incomplete_maps:
        result['status'] = 'INCOMPLETE'
        result['image_status'] = 'INCOMPLETE'
        result['incomplete_maps'] = incomplete_maps
        receipt_path.write_text(json.dumps(result, indent=1) + '\n')
        print('INCOMPLETE', args.label, 'image qualification', json.dumps(incomplete_maps),
              'semantic PASS; witness totals', json.dumps(positive), flush=True)
        raise SystemExit(INCOMPLETE_EXIT)
    result['status'] = 'PASS'
    result['image_status'] = 'EXACT'
    receipt_path.write_text(json.dumps(result, indent=1) + '\n')
    print('PASS', args.label, 'witness totals', json.dumps(positive), flush=True)


if __name__ == '__main__':
    try:
        main()
    except Exception as exc:
        print('FAIL', exc, file=sys.stderr)
        raise
