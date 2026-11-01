"""XO resume cache tests (unittest, no numerical dependencies or upstream run)."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


class CacheFingerprintTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        oracle = root / 'oracle'
        oracle.mkdir()
        (root / 'UPSTREAM.toml').write_text('[upstream]\ncommit = "fixed"\n')
        source = Path(__file__).with_name('export_engine_xo.py')
        for name in ('export_engine_xo.py', 'engine_cases.py', 'export_engine_e2e.py'):
            (oracle / name).write_bytes(source.with_name(name).read_bytes())
        # Import only the exporter under test; numerical and upstream modules
        # are not needed to hash files and must not run in this unit test.
        dependencies = {
            'common': SimpleNamespace(ROOT=root),
            'numpy': SimpleNamespace(__version__='fixed-numpy'),
            'scipy': SimpleNamespace(__version__='fixed-scipy'),
            'engine_cases': SimpleNamespace(cases=None),
            'export_engine_design': SimpleNamespace(DesignFamily=None),
            'export_engine_e2e': SimpleNamespace(LABELS=(), worker=None),
            'secs': SimpleNamespace(calibration=None),
        }
        spec = importlib.util.spec_from_file_location('export_engine_xo_under_test', source)
        self.exporter = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, dependencies):
            spec.loader.exec_module(self.exporter)
        self.exporter.__file__ = str(oracle / source.name)
        self.oracle = oracle

    def test_input_generator_bytes_invalidate_the_resume_cache(self):
        for name in ('engine_cases.py', 'export_engine_e2e.py'):
            with self.subTest(generator=name):
                before = self.exporter.cache_fingerprint('reference')
                cache = self.oracle / 'reference.json'
                cache.write_text(json.dumps(dict(fingerprint=before, results=['old'], cores='cpu')))
                self.assertEqual(self.exporter.cached_run(cache, 'reference'), (['old'], 'cpu'))
                generator = self.oracle / name
                generator.write_bytes(generator.read_bytes() + b'\n# changed input construction\n')
                self.assertNotEqual(before, self.exporter.cache_fingerprint('reference'), name)
                self.assertIsNone(self.exporter.cached_run(cache, 'reference'))

    def test_identical_sources_and_label_keep_the_fingerprint(self):
        self.assertEqual(self.exporter.cache_fingerprint('reference'),
                         self.exporter.cache_fingerprint('reference'))
        self.assertNotEqual(self.exporter.cache_fingerprint('reference'),
                            self.exporter.cache_fingerprint('ulp_0'))


if __name__ == '__main__':
    unittest.main()
