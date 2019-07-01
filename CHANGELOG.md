# Changelog for Vector v0.3.0-dev

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## v0.3.0-dev

### Added

- [`regex_parser` transform] Added the `types` option to coerce captured fields [#542]
- [`tokenizer` transform] Added the `types` option to coerce extracted fields [#547]
- [Docs] Added an install.sh script that can be run via `https://sh.vector.dev` [#549]
- [Workflow] Release to Debian, Untuntu, and Homebrew package managers [#530]

### Changed

- [`aws_s3` sink] `key_prefix` accepts strftime specifiers for dynamic time-based partitioning [#463]
- [`elasticsearch` sink] `index` accepts strftime specifiers for dynamic time-based partitioning [#463]
- [`file` source] Identify file by fingerprint [#535]
- [`log_to_metric` transform] Updated the configuration interface to take a `metrics` table [#539]

## v0.2.X

The CHANGELOG for v0.2.X releases can be found in the [v0.2 branch](https://github.com/timberio/vector/blob/v0.2/CHANGELOG.md).
