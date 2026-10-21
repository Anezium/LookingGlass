#!/usr/bin/env python3
"""Host-only semantic Python mutants in exclusive copied directories.

Run after fresh Rust feature fixtures. A syntax/import/setup failure is NOT a
mutant kill: each baseline target must pass, then the mutant must fail an
assertion in that target. Production sources and original exports stay intact.
"""
import argparse
import json
import os
from pathlib import Path
import py_compile
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
# name, copied production module, exact replacement, asserting test target
MUTANTS = [
    ('ignore-clock-tolerance', 'native_timing_pair_check.py', 'limit, worst <= limit)',
     'limit, True if not ticks else worst <= limit)', 'PairTests.test_one_f32_beyond_clock_limit_fails'),
    ('ignore-tick-tolerance', 'native_timing_pair_check.py', 'limit, worst <= limit)',
     'limit, True if ticks else worst <= limit)', 'PairTests.test_seven_physics_ticks_fail'),
    ('residence-instead-of-simulation', 'native_timing_pair_check.py', 'worst = interval_difference(*ranges)',
     "worst = abs((a.start_us-a.world['ready_us'])-(b.start_us-b.world['ready_us']))/1_000_000 if key != 'physics_ticks' else interval_difference(*ranges)",
     'PairTests.test_equal_wall_age_mismatched_environment_age'),
    ('skip-S-to-T', 'native_timing_pair_check.py', "for edge in ('S', 'T', 'S_to_T'):",
     "for edge in ('S', 'T'):", 'PairTests.test_S_to_T_advance_is_checked_independently'),
    ('bracket-midpoints', 'native_timing_pair_check.py', 'return max(abs(a[0]-b[1]), abs(a[1]-b[0]))',
     'return abs(sum(a)/2-sum(b)/2)', 'PairTests.test_wide_clock_brackets_fail_even_when_identical'),
    ('pose-signed-zero-equality', 'native_timing_pair_check.py', 'poses[0] == poses[1])',
     '[c.f32(v) for v in poses[0]] == [c.f32(v) for v in poses[1]])',
     'PairTests.test_signed_zero_pose_bits_are_distinct'),
    ('skip-boundary-frames', 'native_timing_pair_check.py',
     "durations = sorted(f['end_us'] - f['begin_us'] for f in validated.population)",
     "durations = sorted(f['end_us'] - f['begin_us'] for f in validated.population if f['begin_us'] >= validated.start_us and f['end_us'] <= validated.end_us)",
     'PairTests.test_positive_straddling_pair_uses_worst_case_brackets'),
    ('skip-boundary-fences', 'native_timing_audio_check.py', "if value > 8.0]",
     "if value > 8.0 and i not in observations['boundary_fence_ids']]", 'PairTests.test_left_boundary_fence_kept'),
    ('skip-music-counter', 'native_timing_pair_check.py',
     "for key in ('PcmStarvedBlocks', 'MusicUnderrunFrames'):", "for key in ('PcmStarvedBlocks',):",
     'PairTests.test_nonzero_raw_music_surrounding_delta'),
    ('skip-pcm-counter', 'native_timing_pair_check.py',
     "for key in ('PcmStarvedBlocks', 'MusicUnderrunFrames'):", "for key in ('MusicUnderrunFrames',):",
     'PairTests.test_nonzero_raw_pcm_surrounding_delta'),
    ('accept-null-provenance', 'native_timing_pair_check.py', "external_evidence_gate=a.gate('external')",
     "external_evidence_gate='PASS'", 'PairTests.test_missing_evidence_is_unknown'),
    ('qualify-campaign-from-pair', 'native_timing_pair_check.py',
     'method_qualified=False, campaign_qualified=False, candidate_benefit_established=False,',
     'method_qualified=False, campaign_qualified=True, candidate_benefit_established=False,',
     'PairTests.test_positive_pair_never_qualifies_method_or_campaign'),
    ('producer-80ms-limit', 'native_timing_audio_check.py', 'if value > 8.0]',
     'if value > 80.0]', 'PairTests.test_candidate_audio_fail_cannot_be_excused'),
    ('producer-rule-for-frame-p95', 'native_timing_pair_check.py', 'durations[(95*n + 99)//100 - 1]',
     'durations[(n-1)*95//100]', 'ObservationAndMetricTests.test_hand_calculated_odd_even_quantiles_and_slow_ties'),
    ('slow-frame-greater-or-equal', 'native_timing_pair_check.py', 'value > 100_000 for value',
     'value >= 100_000 for value', 'ObservationAndMetricTests.test_hand_calculated_odd_even_quantiles_and_slow_ties'),
    ('ignore-max-margin', 'native_timing_pair_check.py', "('max_us', spec['limits']['max_extra_us'])",
     "('max_us', 50000)", 'ObservationAndMetricTests.test_performance_margins_exact_then_above'),
    ('ignore-slow-rate', 'native_timing_pair_check.py',
     "mb['above_100ms_count']*ma['N'] <= ma['above_100ms_count']*mb['N'])",
     'True)', 'ObservationAndMetricTests.test_slow_frame_count_and_rate_are_independent'),
    ('nested-config-bool-integer', 'native_timing_pair_check.py',
     'if type(a) is not type(b):', 'if False:', 'ObservationAndMetricTests.test_nested_config_types_are_exact'),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    fixture_root = Path(os.environ['ALICE_TIMING_TEST_OUTPUT']).resolve()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    results = []
    targets = sorted(set(row[4] for row in MUTANTS))
    baseline = out/'baseline'
    baseline.mkdir()
    for source in ROOT.glob('native_timing*.py'):
        shutil.copy2(source, baseline/source.name)
    shutil.copy2(ROOT/'test_native_timing_pair_check.py', baseline/'test_native_timing_pair_check.py')

    def run(directory, tests):
        env = dict(os.environ, ALICE_TIMING_TEST_OUTPUT=str(fixture_root),
                   PAIR_TEST_CASES=str(directory/'cases'), PYTHONDONTWRITEBYTECODE='1')
        argv = [sys.executable, '-B', '-m', 'unittest', '-v'] + [
            'test_native_timing_pair_check.'+test for test in tests]
        completed = subprocess.run(argv, cwd=directory, env=env, capture_output=True, text=True)
        (directory/'run.log').write_text(completed.stdout+completed.stderr, encoding='utf-8')
        (directory/'command.json').write_text(json.dumps(dict(argv=argv, exit=completed.returncode,
            fixture_root=str(fixture_root)), indent=2)+'\n', encoding='utf-8')
        return completed

    before = run(baseline, targets)
    if before.returncode:
        raise RuntimeError('baseline targets failed; see '+str(baseline/'run.log'))
    results.append(dict(name='baseline', tests=len(targets), exit=before.returncode, passed=True))
    (out/'RESULT.json').write_text(json.dumps(results, indent=2)+'\n', encoding='utf-8')
    print('baseline: PASS ('+str(len(targets))+' tests)', flush=True)
    for name, module, old, new, target in MUTANTS:
        directory = out/name
        directory.mkdir()
        for source in baseline.glob('*.py'):
            shutil.copy2(source, directory/source.name)
        source = directory/module
        text = source.read_text(encoding='utf-8')
        if text.count(old) != 1:
            raise RuntimeError('mutant anchor count: '+name)
        source.write_text(text.replace(old, new, 1), encoding='utf-8')
        # Compile errors are hard harness failures, never counted as kills.
        py_compile.compile(str(source), cfile=str(directory/'compiled.pyc'), doraise=True)
        after = run(directory, [target])
        output = after.stdout+after.stderr
        killed = after.returncode == 1 and '\nFAIL: '+target.split('.')[1] in output and 'AssertionError' in output and '\nERROR:' not in output
        results.append(dict(name=name, target=target, exit=after.returncode,
                            compiled=True, assertion_killed=killed))
        (out/'RESULT.json').write_text(json.dumps(results, indent=2)+'\n', encoding='utf-8')
        print(name+(': KILLED' if killed else ': SURVIVED/ERROR'), flush=True)
        if not killed:
            raise RuntimeError('mutant did not fail its assertion; see '+str(directory/'run.log'))


if __name__ == '__main__':
    main()
