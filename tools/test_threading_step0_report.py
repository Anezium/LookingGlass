#!/usr/bin/env python3
import csv
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('report', Path(__file__).with_name('threading-step0-report.py'))
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)

class CsvContract(unittest.TestCase):
    def fixture(self, root, columns=None, frames=range(61,181), probe=False):
        columns = report.COLUMNS if columns is None else columns
        with (root/'window-light-180.csv').open('w',newline='') as stream:
            writer = csv.writer(stream); writer.writerow(columns)
            for frame in frames:
                values = {k:0 for k in columns}
                values.update(frame=frame,frame_cpu_us=100)
                if probe:
                    values.update(sim_blocks_us=10,input_us=10,gpu_probe_frames=1,gpu_clock_reads=2,
                        gpu_vgl_swap_calls=1,gpu_submit_scopes=1,gpu_gxm_begin_scene_calls=1,gpu_gxm_end_scene_calls=1)
                writer.writerow([values[k] for k in columns])
    def test_accepts_complete_both_arms(self):
        for probe in [False, True]:
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp); self.fixture(root,probe=probe)
                self.assertEqual(len(report.read_window(root,probe)[1]),120)
    def test_rejects_missing_duplicate_or_stale_schema(self):
        for columns in [report.COLUMNS[:-1],report.COLUMNS + ['fx_upload_cpu_us'],
                        [k for k in report.COLUMNS if k != 'fx_upload_pack_mixed_us']]:
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp); self.fixture(root,columns)
                with self.assertRaises(ValueError): report.read_window(root,False)
    def test_off_cpu_quantiles_use_per_frame_differences(self):
        rows=[]
        for wall,cpu in [(0,0),(100,99),(100,1)]:
            row={k:0 for k in report.COLUMNS};row.update(gpu_gl_finish_us=wall,gpu_gl_finish_cpu_us=cpu);rows.append(row)
        summary=report.summarize_rows(rows)
        self.assertEqual(summary['gpu_gl_finish_off_cpu_us_estimate']['median'],1)
        self.assertEqual(summary['gpu_gl_finish_off_cpu_us_estimate']['max'],99)
    def test_rejects_inactive_gpu_probe(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); self.fixture(root,probe=True)
            path=root/'window-light-180.csv'
            rows=list(csv.DictReader(path.open()))
            for row in rows: row['gpu_clock_errors']='1'
            with path.open('w',newline='') as stream:
                writer=csv.DictWriter(stream,fieldnames=report.COLUMNS);writer.writeheader();writer.writerows(rows)
            with self.assertRaises(ValueError):report.read_window(root,True)
    def test_rejects_partial_duplicate_and_false_on(self):
        for frames,probe in [(range(61,180),False),([61]*120,False),(range(61,181),True)]:
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp); self.fixture(root,frames=frames)
                with self.assertRaises(ValueError): report.read_window(root,probe)

if __name__ == '__main__': unittest.main()
