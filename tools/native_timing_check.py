#!/usr/bin/env python3
"""Offline phase1 records checker. No legacy CSV/network imports or qualification.

File order is authoritative. Never sort, clip, repair, merge launches or concatenate
cumulative exports. JSON duplicate keys, bool-as-integer and unknown fields reject.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct

SCHEMA = 'alice-native-phase1-v1'
GAPS = ['audio-producer-fences', 'raw-counter-brackets', 'image-tokens', 'review-ack',
        'native-age-automation', 'pair-equivalence', 'complete-qualification']
U64 = 2**64 - 1
MASK = 2**14 - 1

class Invalid(ValueError):
    pass

def need(ok, reason):
    if not ok:
        raise Invalid(reason)

def uint(x, maximum=U64):
    need(type(x) is int and 0 <= x <= maximum, 'invalid unsigned integer')

def boolean(x):
    need(type(x) is bool, 'invalid boolean')

def text(x):
    need(type(x) is str and bool(x), 'invalid string')

def nullable(check):
    def verify(x):
        if x is not None:
            check(x)
    return verify

def array(check, length=None):
    def verify(x):
        need(type(x) is list and (length is None or len(x) == length), 'array type/length')
        for value in x:
            check(value)
    return verify

def obj(x, fields):
    need(type(x) is dict and x.keys() == fields.keys(), 'missing/extra object fields')
    for name, check in fields.items():
        check(x[name])

def literal(expected):
    def check(x):
        need(type(x) is type(expected) and x == expected, 'unexpected constant')
    return check

def u32(x):
    uint(x, 2**32-1)

def octet(x):
    uint(x, 255)

STATE = dict(environment_bits=u32, pickup_bits=u32, physics_ticks=uint,
             pose_bits=array(u32, 5))
SIM = dict(elapsed_bits=u32, dt_bits=u32, world_dt_bits=u32)
FRAME = dict(viewer_seq=uint, measured_id=nullable(uint), begin_us=nullable(uint), end_us=nullable(uint),
             begin_generation=uint, end_generation=uint, begin_operation=uint, end_operation=uint,
             reasons=lambda x: uint(x, MASK), cpu_begin_us=nullable(uint), cpu_end_us=nullable(uint),
             swap_begin_us=nullable(uint), swap_end_us=nullable(uint),
             simulation=nullable(lambda x: obj(x, SIM)), before=lambda x: obj(x, STATE),
             after=nullable(lambda x: obj(x, STATE)))
WORLD = dict(generation=uint, ready_us=nullable(uint), map=text, entry=nullable(text), reason=text,
             baseline=lambda x: obj(x, STATE), save_sha256=nullable(array(octet, 32)))
EVENT = dict(ordinal=uint, native_us=nullable(uint), viewer_seq=uint, generation=uint, operation=uint,
             kind=text, reasons=lambda x: uint(x, MASK))
META = {name: nullable(array(octet, 32)) for name in (
    'source_sha256', 'build_sha256', 'game_config_sha256', 'startup_save_sha256')}
META['recorder_config_sha256'] = array(octet, 32)


def reject_keys(pairs):
    out = {}
    for k, v in pairs:
        need(k not in out, 'duplicate JSON key')
        out[k] = v
    return out

def load(raw):
    try:
        return json.loads(raw, object_pairs_hook=reject_keys,
                          parse_constant=lambda _: (_ for _ in ()).throw(Invalid('nonfinite JSON')))
    except Invalid:
        raise
    except (UnicodeError, ValueError, RecursionError) as error:
        raise Invalid('malformed JSON') from error


def f32(bits):
    result = struct.unpack('<f', struct.pack('<I', bits))[0]
    need(math.isfinite(result), 'nonfinite simulation/state observation')
    return result


def check_capture(directory, generation, age_offset_us):
    directory = Path(directory)
    uint(generation); uint(age_offset_us)
    with (directory/'manifest.json').open('rb') as file:
        manifest_raw = file.read(16*1024+1)
    need(len(manifest_raw) <= 16*1024, 'oversize manifest')
    m = load(manifest_raw)
    mf = {k: uint for k in ('capture_ordinal', 'records_bytes', 'frame_count', 'world_count', 'event_count',
        'viewer_seq', 'completed', 'event_total', 'generation_total', 'operation_total', 'pending_count',
        'frame_capacity', 'world_capacity', 'event_capacity', 'frame_size', 'world_size', 'event_size',
        'recorder_size', 'allocated_storage_bytes', 'aux_reserve_bytes', 'budget_bytes',
        'lost_frames', 'lost_worlds', 'lost_events')}
    mf.update(schema=literal(SCHEMA), complete=literal(True), run_id=nullable(array(octet,16)),
        capture_id=text, clock_id=text, clock_unit=literal('us'), clock_resolution=literal('Vita source microseconds; host quantized to us'),
        clock_lifetime=literal('Vita process lifetime OR host shared Instant session; NEVER join launches or UTC'),
        records_file=literal('records.json'), records_sha256=text, first_viewer_seq=nullable(uint),
        last_viewer_seq=nullable(uint), capture_stop_us=nullable(uint), capture_stop_viewer_seq=nullable(uint), stopped_at_actual_begin=boolean, clock_fault=nullable(text),
        arithmetic_overflow=boolean, invalid_identity=boolean, metadata=lambda x: obj(x, META),
        metadata_assurance=literal('deployment supplied source/build/game-config/start-save associations UNVERIFIED; null UNKNOWN'),
        repeated_cumulative_export=boolean,
        swap_observations=literal('UNAVAILABLE; never estimated from wall minus CPU'),
        door_observations=literal('UNIMPLEMENTED per-frame; legacy door chunks remain separate/unjoined'),
        phase2_unimplemented=literal(GAPS), campaign_qualified=literal(False))
    obj(m, mf)
    need(m['run_id'] is not None and m['run_id'] != [0]*16 and not m['invalid_identity'], 'UNKNOWN run/world identity')
    need(m['clock_id'] in ('vita-newlib-clock4-process-us', 'host-shared-Instant-session-us'), 'clock domain')
    expected_id = bytes(m['run_id']).hex() + '-0000000000000001'
    need(m['capture_ordinal'] == 1 and not m['repeated_cumulative_export'], 'repeated cumulative export')
    need(m['capture_id'] == expected_id and directory.name == expected_id, 'capture identity/path')
    need(not m['clock_fault'] and not m['arithmetic_overflow'], 'clock/arithmetic failure')
    need(m['pending_count'] == 0 and m['stopped_at_actual_begin'], 'unresolved pending interval')
    need(all(m[k] == 0 for k in ('lost_frames', 'lost_worlds', 'lost_events')), 'record loss')
    need(m['frame_capacity'] == 6144 and m['world_capacity'] == 128 and m['event_capacity'] == 512, 'capacity contract')
    need(m['budget_bytes'] == 2*1024*1024 and m['aux_reserve_bytes'] == 65536, 'budget contract')
    size = (m['recorder_size'] + m['frame_capacity']*m['frame_size'] +
            m['world_capacity']*m['world_size'] + m['event_capacity']*m['event_size'])
    need(size == m['allocated_storage_bytes'] and size + m['aux_reserve_bytes'] <= m['budget_bytes'], 'allocation budget')
    need(0 < m['records_bytes'] <= 16*1024*1024, 'payload length cap')
    with (directory/'records.json').open('rb') as file:
        raw = file.read(m['records_bytes']+1)
    need(len(raw) == m['records_bytes'] and hashlib.sha256(raw).hexdigest() == m['records_sha256'], 'payload hash/length')
    d = load(raw)
    obj(d, dict(schema=literal(SCHEMA), run_id=array(octet,16), clock_id=text, capture_ordinal=uint,
        frames=array(lambda x: obj(x, FRAME)), worlds=array(lambda x: obj(x, WORLD)),
        events=array(lambda x: obj(x, EVENT)), pending=nullable(lambda x: obj(x, FRAME))))
    need(d['run_id'] == m['run_id'] and d['clock_id'] == m['clock_id'] and d['capture_ordinal'] == 1, 'payload identity')
    need(d['pending'] is None, 'payload pending interval')
    frames, worlds, events = d['frames'], d['worlds'], d['events']
    for rows, count, total, cap in ((frames,'frame_count','completed','frame_capacity'),
                                   (worlds,'world_count','generation_total','world_capacity'),
                                   (events,'event_count','event_total','event_capacity')):
        need(0 < len(rows) == m[count] == m[total] < m[cap], 'count/saturation/loss')
    need(m['viewer_seq'] >= len(frames)+1 and m['capture_stop_viewer_seq'] == len(frames)+1 and m['capture_stop_us'] == frames[-1]['end_us'] and m['first_viewer_seq'] == 1 and m['last_viewer_seq'] == len(frames), 'sequence manifest')
    previous = None
    for i, w in enumerate(worlds, 1):
        need(w['generation'] == i and w['ready_us'] is not None, 'world generation/ready')
        need(w['reason'] in ('Fresh','StartSave','Load','Retry','ChapterOrConsole','Natural'), 'install reason')
        need(len(w['map'].encode()) <= 64 and (w['entry'] is None or len(w['entry'].encode()) <= 128), 'world identity bounds')
        need(previous is None or w['ready_us'] >= previous, 'world clock backwards')
        previous = w['ready_us']
    for state in [w['baseline'] for w in worlds]:
        for bits in [state['environment_bits'], state['pickup_bits'], *state['pose_bits']]:
            f32(bits)
    previous_event = None
    kinds = {'InstallBegin','Ready','FirstPlayable','Status','ModalEnter','ModalExit','OpeningEnter','OpeningExit',
             'EndingEnter','EndingExit','SkipComplete','TransitionBegin','LoadAttempt','LoadFailed','StopRequested'}
    for i, event in enumerate(events, 1):
        need(event['ordinal'] == i and event['native_us'] is not None and event['kind'] in kinds, 'event sequence/clock/kind')
        need(event['generation'] <= len(worlds) and event['viewer_seq'] <= m['viewer_seq'] and
             event['operation'] <= m['operation_total'], 'event identity')
        if previous_event:
            need(event['native_us'] >= previous_event['native_us'] and event['operation'] >= previous_event['operation'], 'event backwards')
        previous_event = event
    for w in worlds:
        ready = [e for e in events if e['kind'] == 'Ready' and e['generation'] == w['generation']]
        need(len(ready) == 1 and ready[0]['native_us'] >= w['ready_us'], 'missing/duplicate world-ready milestone')
    previous = None
    for i, f in enumerate(frames, 1):
        b, e = f['begin_us'], f['end_us']
        need(f['viewer_seq'] == i and b is not None and e is not None and b < e, 'frame sequence/clock/duration')
        need(1 <= f['begin_generation'] <= f['end_generation'] <= len(worlds), 'frame generation')
        need(worlds[f['begin_generation']-1]['ready_us'] <= b, 'frame before ready')
        need(f['begin_operation'] <= f['end_operation'] <= m['operation_total'], 'frame operation')
        if previous:
            need(previous['end_us'] == b and previous['end_generation'] == f['begin_generation'] and
                 previous['end_operation'] == f['begin_operation'], 'hidden interval gap/discontinuity')
        if f['begin_generation'] != f['end_generation']:
            need(f['reasons'] & 32, 'unmarked install crossing')
        if f['begin_operation'] != f['end_operation']:
            need(f['reasons'] & 64, 'unmarked operation crossing')
        need(f['swap_begin_us'] is None and f['swap_end_us'] is None, 'unsupported swap observations')
        for key in ('cpu_begin_us','cpu_end_us'):
            need(f[key] is None or b <= f[key] <= e, 'CPU outside whole viewer interval')
        if f['cpu_begin_us'] is not None and f['cpu_end_us'] is not None:
            need(f['cpu_begin_us'] <= f['cpu_end_us'], 'CPU backwards')
        if f['reasons'] == 0:
            need(f['cpu_begin_us'] is not None and f['cpu_end_us'] is not None and
                 f['simulation'] is not None and f['after'] is not None, 'missing gameplay observation')
        if f['simulation']:
            sim = f['simulation']
            f32(sim['elapsed_bits'])
            need(0 <= f32(sim['dt_bits']) <= f32(0x3d4ccccd) and f32(sim['world_dt_bits']) >= 0, 'simulation dt observation')
        for state in (f['before'], f['after']):
            if state:
                for bits in [state['environment_bits'], state['pickup_bits'], *state['pose_bits']]:
                    f32(bits)
        if f['after']:
            need(f['after']['physics_ticks'] >= f['before']['physics_ticks'], 'physics counter backwards')
        previous = f
    need(1 <= generation <= len(worlds), 'missing selected world')
    world = worlds[generation-1]
    s = world['ready_us'] + age_offset_us
    t = s + 30_000_000
    need(s >= 10_000_000 and t <= U64, 'window arithmetic/settle')
    settle = s - 10_000_000
    need(frames[0]['end_us'] <= settle and frames[-1]['begin_us'] >= t, 'missing complete guards around settle/window')
    population = [f for f in frames if f['begin_us'] < t and f['end_us'] > s]
    continuous = [f for f in frames if f['begin_us'] < t and f['end_us'] > settle]
    need(population and population[0]['begin_us'] <= s and population[-1]['end_us'] >= t, 'window coverage')
    for f in continuous:
        need(f['begin_generation'] == f['end_generation'] == generation and f['reasons'] == 0 and
             f['begin_operation'] == f['end_operation'], 'non-gameplay/generation/operation during settle/window')
    need(len({f['begin_operation'] for f in continuous}) == 1, 'operation changed during continuous population')
    need(not any(settle <= e['native_us'] < t for e in events), 'milestone/operation inside settle/window')
    playable = [e for e in events if e['kind'] == 'FirstPlayable' and e['generation'] == generation]
    need(len(playable) == 1 and playable[0]['native_us'] < settle, 'missing/late/duplicate first-playable epoch')
    # Factual simulation boundaries only. No exact simulation-age/pair claim.
    return dict(source_population_contract=True, campaign_qualified=False, phase2_unimplemented=GAPS,
        metadata_assurance=m['metadata_assurance'], native_start_us=s, native_end_us=t,
        generation=generation, world=world, N=len(population), small_sample=len(population)<120,
        selected_viewer_seq=[f['viewer_seq'] for f in population],
        boundary_viewer_seq=[f['viewer_seq'] for f in population if f['begin_us'] < s or f['end_us'] > t],
        fully_inside_viewer_seq=[f['viewer_seq'] for f in population if s <= f['begin_us'] and f['end_us'] <= t],
        left_overhang_us=s-population[0]['begin_us'], right_overhang_us=population[-1]['end_us']-t,
        window_us=30_000_000, sum_full_frame_us=sum(f['end_us']-f['begin_us'] for f in population),
        simulation_at_first_begin=population[0]['before'], simulation_at_last_end=population[-1]['after'],
        simulation_age_equivalence='UNKNOWN; no interpolation/retiming', audio_qualification='UNIMPLEMENTED',
        image_qualification='UNIMPLEMENTED', door_totals='UNKNOWN; no clipped/prorated legacy chunks')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('capture', type=Path)
    parser.add_argument('--generation', required=True, type=int)
    parser.add_argument('--age-offset-us', required=True, type=int)
    args = parser.parse_args()
    try:
        result = check_capture(args.capture, args.generation, args.age_offset_us)
    except (Invalid, OSError, OverflowError) as error:
        parser.exit(2, f'UNQUALIFIED phase1 records: {error}\n')
    print(json.dumps(result, indent=2))

if __name__ == '__main__':
    main()
