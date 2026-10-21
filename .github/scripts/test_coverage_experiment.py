import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('coverage_experiment', Path(__file__).with_name('coverage-experiment.py'))
coverage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(coverage)


class PublicationContract(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'reports'
        self.expected = {'coverage-unit': 'unit', 'coverage-int-demo': 'integration'}
        self.sha = 'a' * 40
        self.run_id = '123'
        for artifact, hits in [('coverage-unit', [1, 0, 0]), ('coverage-int-demo', [0, 1, 0])]:
            folder = self.root / artifact
            folder.mkdir(parents=True)
            report = folder / 'lcov.info'
            report.write_text('SF:lib/demo.rs\n' + ''.join(f'DA:{i},{hit}\n' for i, hit in enumerate(hits, 1)) + 'end_of_record\n')
            with patch.object(coverage, 'git', return_value=self.sha):
                coverage.seal(report, artifact, self.run_id)

    def assert_blocked(self):
        with patch.object(coverage, 'git', return_value=self.sha), \
             patch.object(coverage, 'inventory', return_value=self.expected), \
             patch.object(coverage.subprocess, 'run') as uploader:
            with self.assertRaises((ValueError, FileNotFoundError)):
                coverage.publish(self.root, self.sha, self.run_id, Path(self.temp.name) / 'evidence', upload=True)
            uploader.assert_not_called()

    def test_union_deduplicates_lines_and_preserves_flags(self):
        groups, reports = coverage.validate(self.root, self.sha, self.run_id, self.expected)
        self.assertEqual(len(reports), 2)
        self.assertEqual(coverage.summarize(groups['unit'])['covered'], 1)
        self.assertEqual(coverage.summarize(groups['integration'])['covered'], 1)
        self.assertEqual(coverage.summarize(groups['publisher-experiment'])['covered'], 2)
        self.assertEqual(coverage.summarize(groups['publisher-experiment'])['executable'], 3)

    def test_missing_artifact_blocks_all_uploads(self):
        self.expected['coverage-int-missing'] = 'integration'
        self.assert_blocked()

    def test_missing_report_blocks_all_uploads(self):
        (self.root / 'coverage-int-demo/lcov.info').unlink()
        self.assert_blocked()

    def test_wrong_sha_blocks_all_uploads(self):
        path = self.root / 'coverage-int-demo/manifest.json'
        data = json.loads(path.read_text())
        data['sha'] = 'b' * 40
        path.write_text(json.dumps(data))
        self.assert_blocked()

    def test_wrong_run_blocks_all_uploads(self):
        self.run_id = '124'
        self.assert_blocked()

    def test_modified_report_blocks_all_uploads(self):
        path = self.root / 'coverage-int-demo/lcov.info'
        path.write_text(path.read_text().replace('DA:3,0', 'DA:3,1'))
        self.assert_blocked()

    def test_unit_report_without_workspace_libraries_is_rejected(self):
        path = self.root / 'coverage-unit/lcov.info'
        path.write_text(path.read_text().replace('lib/demo.rs', 'src/demo.rs'))
        with patch.object(coverage, 'git', return_value=self.sha):
            coverage.seal(path, 'coverage-unit', self.run_id)
        self.assert_blocked()

    def test_absolute_path_and_truncated_report_are_rejected(self):
        path = Path(self.temp.name) / 'bad.info'
        for contents in ['SF:/home/vector/src/a.rs\nDA:1,1\nend_of_record\n',
                         'SF:src/a.rs\nDA:1,1\n', '']:
            path.write_text(contents)
            with self.assertRaises(ValueError):
                coverage.read_lcov(path)

    def test_publisher_passes_original_reports_and_explicit_identity(self):
        self.expected['coverage-e2e-demo'] = 'e2e-runner'
        folder = self.root / 'coverage-e2e-demo'
        folder.mkdir()
        report = folder / 'lcov.info'
        report.write_text('SF:lib/demo.rs\nDA:1,0\nDA:2,0\nDA:3,1\nend_of_record\n')
        with patch.object(coverage, 'git', return_value=self.sha):
            coverage.seal(report, 'coverage-e2e-demo', self.run_id)
        with patch.object(coverage, 'git', return_value=self.sha), \
             patch.object(coverage, 'inventory', return_value=self.expected), \
             patch.dict(os.environ, {'COVERAGE_BRANCH': 'experiment'}), \
             patch.object(coverage.subprocess, 'run') as uploader:
            coverage.publish(self.root, self.sha, self.run_id, Path(self.temp.name) / 'evidence', upload=True)
            self.assertEqual(uploader.call_count, 3)
            for call in uploader.call_args_list:
                self.assertIn('publisher-experiment', call.args[0])
                self.assertTrue(call.args[0][-1].endswith('lcov.info'))
                self.assertEqual(call.kwargs['env']['DD_GIT_COMMIT_SHA'], self.sha)
                self.assertEqual(call.kwargs['env']['DD_GIT_BRANCH'], 'experiment')


if __name__ == '__main__':
    unittest.main()
