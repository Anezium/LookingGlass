"""Offline receipt tests: KEPT saves are compared by read hash, not by the label."""
import hashlib
import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

PACKAGE = Path(__file__).resolve().parent


def load_receipts(root):
    spec = importlib.util.spec_from_file_location('receipts', PACKAGE / 'verify-session.py')
    module = importlib.util.module_from_spec(spec)
    module.__file__ = str(root / 'verify-session.py')
    spec.loader.exec_module(module)
    module.P = root
    return module


def sha(data):
    return hashlib.sha256(data).hexdigest()


class Receipts(unittest.TestCase):
    def fixture(self, root):
        back = root / 'backup.json'
        (root / 'READY.json').write_text(json.dumps(dict(arms=[], self_sha256='0' * 64)))
        self.owner, self.config, self.save = b'owner SELF', b'{"owner": true}', b'save bytes'
        rows = [
            dict(path='ux0:/app/ALICELG01/eboot.bin', sha256=sha(self.owner), kind='self'),
            dict(path='ux0:/data/alice/private/vita_renderer.json', sha256=sha(self.config), kind='config'),
            dict(path='ux0:/data/alice/private/saves/auto.json', sha256=sha(self.save), kind='save'),
        ]
        back.write_text(json.dumps(dict(files=rows, absent=[])))
        (root / 'restore.json').write_text(json.dumps(dict(keep_saves=True, rows=[
            dict(path=rows[0]['path'], expected=rows[0]['sha256'], read=rows[0]['sha256'], result='OK'),
            dict(path=rows[1]['path'], expected=rows[1]['sha256'], read=rows[1]['sha256'], result='OK'),
            dict(path=rows[2]['path'], expected='kept (session save)', read=rows[2]['sha256'], result='KEPT'),
        ])))
        (root / 'state.json').write_text(json.dumps(
            dict(game='closed', pending=None, log=dict(pid=None))))
        return rows

    def run_check(self, module):
        with mock.patch.object(module.subprocess, 'check_output', return_value=''), \
                mock.patch.object(module.subprocess, 'run',
                                  return_value=mock.Mock(returncode=1, stdout='')):
            module.check_shutdown()

    def test_kept_save_passes_by_read_hash(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fixture(root)
            module = load_receipts(root)
            self.run_check(module)
            receipt = json.loads((root / 'shutdown.json').read_text())
            self.assertTrue(receipt['keep_saves'])
            self.assertEqual(receipt['kept_saves'], 1)
            self.assertEqual(receipt['port9999'], 'free')

    def test_kept_label_with_wrong_read_hash_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            rows = self.fixture(root)
            rows[2]['sha256'] = sha(b'other save')
            (root / 'backup.json').write_text(json.dumps(dict(files=rows, absent=[])))
            module = load_receipts(root)
            with self.assertRaises(SystemExit) as error:
                self.run_check(module)
            self.assertIn('KEPT read hash differs', str(error.exception))

    def test_owner_hash_mismatch_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fixture(root)
            restore = json.loads((root / 'restore.json').read_text())
            restore['rows'][1]['read'] = sha(b'other config')
            (root / 'restore.json').write_text(json.dumps(restore))
            module = load_receipts(root)
            with self.assertRaises(SystemExit) as error:
                self.run_check(module)
            self.assertIn('restore read-back differs', str(error.exception))


if __name__ == '__main__':
    unittest.main(verbosity=2)
