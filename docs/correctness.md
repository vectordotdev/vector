# Correctness

This page demonstrates behavior correctness. Like our [performance tests](performance.md), our correct tests use the same Vector [test harness](https://github.com/timberio/vector-test-harness). You can click on the test, if applicable, to access the test's source and learn more about the test itself. This is by no means comprehensive and we plan to expand the tests over time

## Tests

| Test | Vector | Filebeat | FluentBit | FluentD | Logstash | Splunk UF | Splunk HF |
| ---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| [Disk Buffer Persistence](https://github.com/timberio/vector-test-harness/tree/master/cases/disk_buffer_performance) | ✅ | ✅ | ❌ | ❌ | ⚠️\* | ✅ | ✅ |
| [File Rotate \(create\)](https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_create_correctness) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| [File Rotate \(copytruncate\)](https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_truncate_correctness) | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| [File Truncation](https://github.com/timberio/vector-test-harness/tree/master/cases/file_truncate_correctness) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| [Process \(SIGHUP\)](https://github.com/timberio/vector-test-harness/tree/master/cases/sighup_correctness) | ✅ | ❌ | ❌ | ❌ | ⚠️\* | ✅ | ✅ |
| TCP Streaming | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| [JSON \(wrapped\)](https://github.com/timberio/vector-test-harness/tree/master/cases/wrapped_json_correctness) | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ | ✅ |

`*` - please click into the test for an explanation of results

## How It Works

You can learn more about how our correctness tests work by clicking on each test or viewing the `README` in the test hardness repo.

