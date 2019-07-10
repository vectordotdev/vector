
# Changelog for Vector v0.4.0-dev

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## v0.4.0-dev

### Added
- [configuration] Added `--dry-run` option [#233]
- aws_s3: Add `filename_extension` options.
- aws_cloudwatch_logs: `stream_name` now accepts `{{key}}` syntax for extracting values from events.
- aws_cloudwatch_logs: retry support added and more stability improvements
- coercer: New transform to convert fields into specified types.
- file source: `data_dir` now falls back to global `data_dir` option if not specified
- aws_kinesis_streams: Added configurable partition keys
- topology: Added ability to disable individual sink healthchecks
- aws_cloudwatch_logs: Add dynamic group and stream creation
- elasticsearch sink: Add support for custom headers and query parameters
- `file` sink: New sink with templates-based partitioning

### Changed

- [configuration] Empty inputs are treated as errors instead of warnings [#506]
- aws_cloudwatch_logs: Now partitions events by `log_group`/`log_stream`.
- All sinks now return structured events instead of flattened events.
- elasticsearch: `doc_type` is now optional defaulting to `_doc_`.

### Deprecated

### Fixed

- aws_s3: Fixed #517 and trailing slash issues with the generated key.
- aws_cloudwatch_logs: Fixes #586 and now dynamically creates streams if they do not exist.
- topology: Reloading a configuration which removes both a sink and its source now works (#681). 
- config: abort reload on unparsable config

### Removed

### Security

## v0.3.X

The CHANGELOG for v0.3.X releases can be found in the [v0.3 branch](https://github.com/timberio/vector/blob/v0.3/CHANGELOG.md).
