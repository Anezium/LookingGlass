import csv
import tempfile
import unittest
from pathlib import Path
from gpu_split_report import HEADER, NAMES, read, summarize


def fixture(on=True):
    rows = []
    for frame in range(1, 181):
        r = dict.fromkeys(HEADER, 0)
        r.update(frame=frame, wall_us=100)
        if on:
            r.update(gpu_probe_frames=1, gpu_vgl_swap_calls=1, gpu_submit_scopes=2,
                     gpu_clock_reads=8, gpu_submit_cpu_us=20, gpu_pre_swap_cpu_us=40,
                     gpu_gxm_begin_scene_calls=1, gpu_gxm_end_scene_calls=1,
                     gpu_gxm_display_queue_add_calls=1)
        rows.append(r)
    return rows


class Reducer(unittest.TestCase):
    def test_population_windows_and_off(self):
        v = summarize(fixture())
        self.assertEqual([(w['frame_first'], w['frame_last']) for w in v['windows']], [(61,120),(121,180)])
        self.assertEqual(v['selected']['metrics']['gpu_submit_cpu_us']['total_us'], 2400)
        self.assertIsNone(v['gpu_busy_us'])
        summarize(fixture(False), 'off')

    def test_quantiles_of_differences_not_difference_of_quantiles(self):
        rows = fixture()
        for r in rows:
            r.update(gpu_vgl_swap_us=100, gpu_vgl_swap_cpu_us=90 if r['frame'] % 2 else 10)
        v = summarize(rows)['selected']['metrics']['gpu_vgl_swap_off_cpu_us_estimate']
        self.assertEqual((v['median_us'],v['p95_us'],v['total_us']), (90,90,6000))

    def test_rejects_missing_row_clock_swap_coverage_and_probe_off(self):
        for key, value in [('gpu_clock_errors',1),('gpu_probe_frames',0),('gpu_vgl_swap_calls',0),('gpu_submit_scopes',0),('gpu_submit_nonwait_cpu_us',21)]:
            rows=fixture(); rows[61][key]=value
            with self.subTest(key=key), self.assertRaises(ValueError): summarize(rows)
        for key in ('gpu_submit_cpu_us','gpu_pre_swap_cpu_us','gpu_gxm_begin_scene_calls','gpu_gxm_end_scene_calls','gpu_gxm_display_queue_add_calls'):
            rows=fixture()
            for r in rows: r[key]=0
            with self.subTest(key=key), self.assertRaises(ValueError): summarize(rows)
        with self.assertRaises(ValueError): summarize(fixture()[:-1])
        with self.assertRaises(ValueError): summarize(fixture(), 'off')
        with self.assertRaises(ValueError): summarize(fixture(), frames=61)

    def test_strict_schema_and_frame_sequence(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'gpu.csv'
            def write(header, rows):
                with p.open('w',encoding='utf-8',newline='') as f:
                    w=csv.writer(f); w.writerow(header); w.writerows(rows)
            rows=[[r[k] for k in HEADER] for r in fixture()]
            write(HEADER,rows); self.assertEqual(len(read(p)),180)
            for h in (HEADER+['frame'],HEADER[:-1],list(reversed(HEADER))):
                write(h,rows)
                with self.assertRaises(ValueError): read(p)
            rows[1][0]=1; write(HEADER,rows)
            with self.assertRaises(ValueError): read(p)

    def test_rust_export_schema_matches_reducer(self):
        import re
        s=(Path(__file__).parents[1]/'src/gpu_wait.rs').read_text(encoding='utf-8')
        schema=s.split('pub(crate) const NAMES:')[1].split('];',1)[0]
        self.assertEqual(re.findall(r'"(gpu_[^"]+)"',schema),NAMES)


if __name__ == '__main__': unittest.main()
