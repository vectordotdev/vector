#!/usr/bin/env python3
"""Temporary CI experiment: validate original reports before publishing to Datadog."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import subprocess


def git(*args):
    return subprocess.check_output(['git', *args], text=True).strip()


def read_lcov(path):
    files = {}
    current = None
    records = 0
    for raw in path.read_text().splitlines():
        if raw.startswith('SF:'):
            if current is not None:
                raise ValueError(f'{path}: unterminated record')
            current = raw[3:]
            source = PurePosixPath(current)
            if source.is_absolute() or '..' in source.parts or '\\' in current:
                raise ValueError(f'{path}: source path must be repository-relative: {current}')
            files.setdefault(current, {})
        elif raw.startswith('DA:'):
            if current is None:
                raise ValueError(f'{path}: line outside a source record')
            line, count, *_ = raw[3:].split(',')
            line, count = int(line), int(count)
            if line < 1 or count < 0:
                raise ValueError(f'{path}: invalid line/count')
            lines = files[current]
            lines[line] = lines.get(line, False) or count > 0
        elif raw == 'end_of_record':
            if current is None:
                raise ValueError(f'{path}: unexpected record end')
            current = None
            records += 1
    if current is not None or not records or not any(files.values()):
        raise ValueError(f'{path}: incomplete or empty LCOV report')
    return files


def seal(report, artifact, run_id):
    read_lcov(report)
    manifest = dict(version=1, artifact=artifact, sha=git('rev-parse', 'HEAD'),
                    run_id=run_id, report=report.name,
                    sha256=hashlib.sha256(report.read_bytes()).hexdigest())
    report.with_name('manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')


def inventory():
    # Read the actual workflow matrix, so the experiment has no duplicate service list.
    import yaml
    jobs = yaml.safe_load(Path('.github/workflows/integration.yml').read_text())['jobs']
    expected = {'coverage-unit': 'unit'}
    for job, prefix, flag in [('integration-tests', 'int', 'integration'),
                              ('e2e-tests', 'e2e', 'e2e-runner')]:
        for service in jobs[job]['strategy']['matrix']['service']:
            expected[f'coverage-{prefix}-{service}'] = flag
    return expected


def union(target, source):
    for name, lines in source.items():
        dest = target.setdefault(name, {})
        for line, covered in lines.items():
            dest[line] = dest.get(line, False) or covered


def summarize(files):
    executable = sum(len(lines) for lines in files.values())
    covered = sum(sum(lines.values()) for lines in files.values())
    return dict(files=len(files), executable=executable, covered=covered,
                percentage=100 * covered / executable if executable else None)


def validate(root, sha, run_id, expected):
    actual = {p.name for p in root.iterdir() if p.is_dir()}
    if actual != set(expected):
        raise ValueError(f'artifact inventory mismatch: missing={sorted(set(expected)-actual)}, '
                         f'unexpected={sorted(actual-set(expected))}')
    groups = {}
    all_files = {}
    reports = {}
    for artifact, flag in sorted(expected.items()):
        folder = root / artifact
        manifest = json.loads((folder / 'manifest.json').read_text())
        for key, value in dict(version=1, artifact=artifact, sha=sha, run_id=run_id,
                               report='lcov.info').items():
            if manifest.get(key) != value:
                raise ValueError(f'{artifact}: {key} mismatch')
        report = folder / 'lcov.info'
        if {p.name for p in folder.iterdir()} != {'lcov.info', 'manifest.json'}:
            raise ValueError(f'{artifact}: unexpected or missing files')
        if hashlib.sha256(report.read_bytes()).hexdigest() != manifest['sha256']:
            raise ValueError(f'{artifact}: report checksum mismatch')
        files = read_lcov(report)
        if artifact == 'coverage-unit' and not any(p.startswith('lib/') for p in files):
            raise ValueError('unit report is missing workspace library coverage')
        union(groups.setdefault(flag, {}), files)
        union(all_files, files)
        reports[artifact] = summarize(files)
    groups['publisher-experiment'] = all_files
    return groups, reports


def publish(root, sha, run_id, output, upload=False):
    if git('rev-parse', 'HEAD') != sha:
        raise ValueError('publisher checkout does not match tested SHA')
    expected = inventory()
    groups, reports = validate(root, sha, run_id, expected)
    output.mkdir(parents=True, exist_ok=True)
    summary = dict(sha=sha, run_id=run_id, reports=reports,
                   groups={flag: summarize(files) for flag, files in groups.items()})
    (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    # Exact line sets support an independent comparison against Datadog's files API.
    (output / 'expected-lines.json').write_text(json.dumps({flag: {
        name: dict(executable=sorted(lines), covered=sorted(n for n, hit in lines.items() if hit))
        for name, lines in files.items()} for flag, files in groups.items()}))
    print(json.dumps(summary['groups'], indent=2))
    if upload:
        env = dict(os.environ, DD_GIT_COMMIT_SHA=sha,
                   DD_GIT_REPOSITORY_URL='https://github.com/vectordotdev/vector',
                   DD_GIT_BRANCH=os.environ['COVERAGE_BRANCH'])
        for flag in ('unit', 'integration', 'e2e-runner'):
            paths = [str(root / artifact / 'lcov.info')
                     for artifact, group in expected.items() if group == flag]
            subprocess.run(['datadog-ci', 'coverage', 'upload', '--flags', flag,
                            '--flags', 'publisher-experiment', *paths], env=env, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    seal_parser = sub.add_parser('seal')
    seal_parser.add_argument('report', type=Path)
    seal_parser.add_argument('artifact')
    seal_parser.add_argument('--run-id', default=os.environ.get('GITHUB_RUN_ID'), required=False)
    pub = sub.add_parser('publish')
    pub.add_argument('root', type=Path)
    pub.add_argument('--sha', required=True)
    pub.add_argument('--run-id', required=True)
    pub.add_argument('--output', type=Path, default=Path('coverage-evidence'))
    pub.add_argument('--upload', action='store_true')
    args = parser.parse_args()
    if args.command == 'seal':
        if not args.run_id:
            parser.error('run ID is required')
        seal(args.report, args.artifact, args.run_id)
    else:
        publish(args.root, args.sha, args.run_id, args.output, args.upload)


if __name__ == '__main__':
    main()
