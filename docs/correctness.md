# Correctness

Like our [performance tests][docs.performance], our correctness tests use the
same [test harness][url.test_harness]. To learn more, click on each test, or
visit the ["How It Works" section][url.test_harness.how-it-works] in the test
harness README.

## Tests

| Test | Vector | Filebeat | FluentBit | FluentD | Logstash | Splunk UF | Splunk HF |
| ---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| [Disk Buffer Persistence][url.disk_buffer_persistence_correctness_test] | ✅ | ✅ | ❌ | ❌ | ⚠️ | ✅ | ✅ |
| [File Rotate \(create\)][url.file_rotate_create_correctness_test] | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| [File Rotate \(copytruncate\)][url.file_rotate_truncate_correctness_test] | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| [File Truncation][url.file_truncate_correctness_test] | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| [Process \(SIGHUP\)][url.sighup_correctness_test] | ✅ | ❌ | ❌ | ❌ | ⚠️ | ✅ | ✅ |
| TCP Streaming | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| [JSON \(wrapped\)][url.wrapped_json_correctness_test] | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ | ✅ |

✅ = passed<br />
❌ = failed<br />
⚠️ = unknown, please click into the test for an explanation


[docs.performance]: /performance.md
[url.disk_buffer_persistence_correctness_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/disk_buffer_persistence_correctness
[url.file_rotate_create_correctness_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_create_correctness
[url.file_rotate_truncate_correctness_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_truncate_correctness
[url.file_truncate_correctness_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/file_truncate_correctness
[url.sighup_correctness_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/sighup_correctness
[url.test_harness.how-it-works]: https://github.com/timberio/vector-test-harness/#how-it-works
[url.test_harness]: https://github.com/timberio/vector-test-harness/
[url.wrapped_json_correctness_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/wrapped_json_correctness
