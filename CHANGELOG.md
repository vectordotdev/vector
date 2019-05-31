# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## 0.2.0-alpha.1

### Added
  
  - [`aws_cloudwatch_logs` sink] Added `ecoding` option that accepts `json` and `text` [pr#352]
  - [data model] Added `metric` type [pr#374]
  - [`http` sink] Added `encoding` option that accepts `ndjson` and `text` [pr#373]
  - [sources] Added `statsd` source
  - [sources] Added `prometheus` source
  - [transforms] Added `log_to_metric` transform [pr#374]
  - [transforms] Added `lua` transform [pr#330]

### Changed

  - [`aws_s3` sink] Renamed from `s3` to `aws_s3` [pr#376]
  - [`aws_cloudwatch_logs` sink] Renamed from `cloudwatch` to `aws_cloudwatch_logs` [pr#376]
  - [`aws_cloudwatch_logs` sink] Dynamically encodes data based on the implicit structuring of the event [pr#352]
  - [`aws_kinesis_stream` sink] Renamed from `kinesis` to `aws_kinesis_streams` [pr#376]
  - [`file` source] Automatically adds the `"host"` context key [pr#372]
  - [http] Update HttpRetryLogic to retry 429 and not retry 501 [pr#375]
  - [`stdin` source] Automatically adds the `"host"` context key [pr#372]


[pr#330]: https://github.com/timberio/vector/pull/330
[pr#352]: https://github.com/timberio/vector/pull/352
[pr#372]: https://github.com/timberio/vector/pull/372
[pr#373]: https://github.com/timberio/vector/pull/373
[pr#374]: https://github.com/timberio/vector/pull/374
[pr#375]: https://github.com/timberio/vector/pull/375
[pr#376]: https://github.com/timberio/vector/pull/376