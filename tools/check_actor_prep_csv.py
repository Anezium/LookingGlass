#!/usr/bin/env python3
"""Validate J23 light-profiler frame identity, exclusive fields and body bounds."""
import argparse
import csv
from pathlib import Path

CATEGORIES = """input interactions movers movement story particles combat alice audio camera
scene gather actor_discovery_setup overlap_consume_validation_copy simple_layer_publication
 dynamic_modifiers packet_publication collect_wait finish_bookkeeping upload actual_submission
other actor_shadow_atmosphere""".split()
FIELDS = ['exclusive_ns', 'calls', 'copied_bytes', 'compared_bytes', 'logical_draws']
COLUMNS = [f'actor_prep_{name}_{field}' for name in CATEGORIES for field in FIELDS]

def validate(paths, enabled):
    rows = []
    for path in paths:
        with path.open(newline='') as stream:
            reader = csv.DictReader(stream)
            assert len(reader.fieldnames) == len(set(reader.fieldnames)) == 241, path
            assert reader.fieldnames[124:] == ['actor_prep_profile_enabled', 'actor_prep_body_ns'] + COLUMNS, path
            rows.extend({key: int(value) for key, value in row.items()} for row in reader)
    assert rows, 'no completed light-profiler windows'
    frames = [row['frame'] for row in rows]
    assert frames == list(range(frames[0], frames[0] + len(frames))), 'noncontiguous/duplicate frames'
    for row in rows:
        assert row['actor_prep_profile_enabled'] == int(enabled), row['frame']
        values = [row[key] for key in COLUMNS]
        assert all(value >= 0 for value in values)
        if enabled:
            total = sum(row[f'actor_prep_{name}_exclusive_ns'] for name in CATEGORIES)
            assert 0 < total <= row['actor_prep_body_ns'], (row['frame'], total, row['actor_prep_body_ns'])
            assert row['actor_prep_body_ns'] // 1000 <= row['frame_cpu_us'], row['frame']
        else:
            assert row['actor_prep_body_ns'] == 0 and not any(values), row['frame']
    if enabled:
        for field in FIELDS:
            assert any(row[f'actor_prep_{name}_{field}'] > 0 for row in rows for name in CATEGORIES), field
    return frames

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--probe', choices=['on', 'off'], required=True)
    parser.add_argument('csv', nargs='+', type=Path)
    args = parser.parse_args()
    frames = validate(args.csv, args.probe == 'on')
    print(f'PASS probe={args.probe}: {len(frames)} contiguous frames {frames[0]}-{frames[-1]}, 241 columns')
