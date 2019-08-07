
# Changelog for Vector v0.4.0-dev

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## v0.4.0-dev

### Added

- [configuration] Added `--dry-run` and `--healthchecks-only` options [#233]
- aws_s3: Add `filename_extension` options.
- aws_cloudwatch_logs: `stream_name` now accepts `{{key}}` synatx for extracting values from events.
- aws_cloudwatch_logs: retry support added and more stablity improvements
- coercer: New transform to convert fields into specified types.
- file source: `data_dir` now falls back to global `data_dir` option if not specified
- aws_kinesis_streams: Added configurable partition keys

### Changed

- [configuration] Empty inputs are treated as errors instead of warnings [#506]
- aws_cloudwatch_logs: Now partitions events by `log_group`/`log_stream`.
- All sinks now return structured events instead of flattened events.
- elasticsearch: `doc_type` is now optional defaulting to `_doc_`.

### Deprecated

### Fixed

- aws_s3: Fixed #517 and trailing slash issues with the generated key.
- aws_cloudwatch_logs: Fixes #586 and now dynamically creates streams if they do not exist.

### Removed

### Security

## v0.3.X

The CHANGELOG for v0.3.X releases can be found in the [v0.3 branch](https://github.com/timberio/vector/blob/v0.3/CHANGELOG.md).
