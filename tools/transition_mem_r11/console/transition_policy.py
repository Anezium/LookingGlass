"""Fixed r11b policy, shared by passive checks, waits and final audit."""
import re
MIB = 1048576
USED_LIMIT = 170 * MIB
ARENA_LIMIT = 179 * MIB
OBSERVED_ARENA_FAILURE = 187994112
LOW_HEADROOM_BYTES = 4299008
LOW_ARENA_PEAK = 175 * MIB
LOAD_TIMEOUT_SECONDS = 100
EXPECTED = [('startup-entry', 'hedge3'), ('native-load', 'pandemonium'),
            ('chapter-or-console', 'hedge3')]

def arena_headroom(text, rows):
    peaks = [int(v) for v in re.findall(r'\bheap_arena=(\d+)', text)]
    for row in rows:
        if row.get('peak_heap_arena') is not None:
            peaks.append(row['peak_heap_arena'])
        peaks.extend(row[k]['heap_arena'] for k in ['before','released','after']
                     if isinstance(row.get(k), dict) and 'heap_arena' in row[k])
    assert peaks, 'Missing arena evidence'
    peak = max(peaks)
    headroom = OBSERVED_ARENA_FAILURE - peak
    return dict(max_observed_heap_arena=peak, arena_headroom_bytes=headroom,
                # The review's byte threshold and its 175 MiB parenthetical
                # are not arithmetically equivalent. Honour both conservatively.
                arena_headroom_label='PASS_LOW_ARENA_HEADROOM' if headroom < LOW_HEADROOM_BYTES or peak >= LOW_ARENA_PEAK else 'PASS',
                arena_failure_reference_bytes=OBSERVED_ARENA_FAILURE)

def check_text_samples(text):
    # Native audio-memory ticks occur outside brackets too. An observed arena
    # ceiling violation anywhere in the arm is a stop, including title/gameplay.
    for value in re.findall(r'\bheap_arena=(\d+)', text):
        assert 0 < int(value) < ARENA_LIMIT, 'Fixed 179 MiB arena budget exceeded'

def check_snapshot(row):
    assert isinstance(row, dict), 'Missing heap/arena sample'
    used, arena, free = (row.get(k) for k in ['heap_used', 'heap_arena', 'heap_free'])
    assert all(isinstance(v, int) and v > 0 for v in [used, arena]), 'Missing heap/arena sample'
    assert isinstance(free, int) and free >= 0, 'Missing fordblks sample'
    assert used <= arena and free <= arena, 'Invalid allocator sample'
    assert used < USED_LIMIT, 'Fixed 170 MiB transition budget exceeded'
    assert arena < ARENA_LIMIT, 'Fixed 179 MiB arena budget exceeded'
    return dict(uordblks=used, arena=arena, fordblks=free, arena_minus_uordblks=arena-used,
                **{k: row.get(k) for k in ['vgl_ram_free', 'vgl_vram_free', 'vgl_phycont_free']})

def check_record(row):
    samples = {'before': check_snapshot(row.get('before'))}
    if row.get('event') == 'end':
        assert row.get('success') is True, 'Unsuccessful transition'
        assert 0 < row.get('peak_heap_used', 0) < USED_LIMIT, 'Fixed 170 MiB transition budget exceeded'
        assert 0 < row.get('peak_heap_arena', 0) < ARENA_LIMIT, 'Fixed 179 MiB arena budget exceeded'
        if row.get('kind') != 'startup-entry':
            samples['released'] = check_snapshot(row.get('released'))
        samples['after'] = check_snapshot(row.get('after'))
        assert row['peak_heap_used'] >= max(v['uordblks'] for v in samples.values()), 'Peak below sample'
        assert row['peak_heap_arena'] >= max(v['arena'] for v in samples.values()), 'Arena peak below sample'
    return samples
