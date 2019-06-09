# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## 0.3.0-dev

## 0.2.0

### Added
  
  - [`aws_cloudwatch_logs` sink] Added `ecoding` option that accepts `json` and `text` - #352
  - [data model] Added `metric` type - #374
  - [`http` sink] Added `encoding` option that accepts `ndjson` and `text` - #373
  - [`http` sink] Added `encoding` option that accepts `ndjson` and `text` - #373
  - [`regex_parser` transform] Added `drop_field` option - #456
  - [sinks] Added `vector` sink - #409
  - [sources] Added `prometheus` source
  - [sources] Added `statsd` source - #311
  - [sources] Added `vector` source - #409
  - [transforms] Added `grok_parser` transform - #455
  - [transforms] Added `log_to_metric` transform - #374
  - [transforms] Added `lua` transform - #330

### Changed

  - [`aws_s3` sink] Renamed from `s3` to `aws_s3` - #376
  - [`aws_cloudwatch_logs` sink] Renamed from `cloudwatch` to `aws_cloudwatch_logs` - #376
  - [`aws_cloudwatch_logs` sink] Dynamically encodes data based on the implicit structuring of the event - #352
  - [`aws_kinesis_stream` sink] Renamed from `kinesis` to `aws_kinesis_streams` - #376
  - [buffers] Improved disk buffer performance - #434
  - [`file` source] Automatically adds the `"host"` context key - #372
  - [http] Update HttpRetryLogic to retry 429 and not retry 501 - #375
  - [sinks] Updated default HTTP retry policy to indefinitely retry - #466
  - [sinks] Updateds sink defaults to be inline with their service - #439
  - [`stdin` source] Automatically adds the `"host"` context key - #372