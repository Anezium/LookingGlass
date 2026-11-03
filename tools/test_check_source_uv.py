#!/usr/bin/env python3
"""The .uv write check of check_source.py: real tree clean, each write form caught, reads ignored."""
from pathlib import Path
import sys, unittest
sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_source as c


class UvWrites(unittest.TestCase):
    def test_every_write_form_outside_the_list_fails(self):
        for line in (b"v.uv = uv;", b"v.uv=uv;", b"v.uv += d;", b"v.uv.x = 0.;", b"v.uv.y*=2.;",
                     b"vertices[0].uv =\n  uv;", b"let u = &mut v.uv;", b"f(&mut mesh.vertices[0].uv);"):
            self.assertEqual(len(c.uv_write_errors("src/npc.rs", b"\n" + line)), 1, line)
        self.assertIn("src/npc.rs:2", c.uv_write_errors("src/npc.rs", b"\nv.uv = uv;")[0])

    def test_reads_and_other_names_pass(self):
        for line in (b"let a = v.uv;", b"v.uv == w.uv", b"(v.uv.x * 10.).sin()", b"f(v.uv, age)", b"v.uv != w.uv", b"out.uvw = 1;"):
            self.assertEqual(c.uv_write_errors("src/npc.rs", line), [], line)

    def test_listed_files_and_other_kinds_pass(self):
        self.assertEqual(c.uv_write_errors("src/weapons.rs", b"v.uv = uv;"), [])
        self.assertEqual(c.uv_write_errors("docs/x.md", b"v.uv = uv;"), [])

    def test_audit_data_reports_it(self):
        self.assertTrue(any(".uv write" in e for e in c.audit_data("src/npc.rs", b"v.uv = uv;")))

    def test_tree_has_no_unaudited_write_and_no_stale_listing(self):
        written = set()
        for p in c.working_files():
            rel = p.relative_to(c.ROOT).as_posix()
            if rel.endswith(".rs"):
                data = p.read_bytes()
                self.assertEqual(c.uv_write_errors(rel, data), [])
                if c.UV_WRITE.search(data):
                    written.add(rel)
        self.assertEqual(written, c.UV_WRITE_FILES)


if __name__ == "__main__":
    unittest.main()
