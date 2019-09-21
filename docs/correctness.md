# Correctness

Like our [performance tests][docs.performance], our correctness tests use the
same [test harness][urls.test_harness]. To learn more, click on each test, or
visit the ["How It Works" section][urls.test_harness#how-it-works] in the test
harness README.

## Tests

| Test | Vector | Filebeat | FluentBit | FluentD | Logstash | Splunk UF | Splunk HF |
| ---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| [Disk Buffer Persistence][urls.disk_buffer_persistence_correctness_test] | ✅ | ✅ | ❌ | ❌ | ⚠️ | ✅ | ✅ |
| [File Rotate \(create\)][urls.file_rotate_create_correctness_test] | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| [File Rotate \(copytruncate\)][urls.file_rotate_truncate_correctness_test] | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| [File Truncation][urls.file_truncate_correctness_test] | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| [Process \(SIGHUP\)][urls.sighup_correctness_test] | ✅ | ❌ | ❌ | ❌ | ⚠️ | ✅ | ✅ |
| TCP Streaming | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| [JSON \(wrapped\)][urls.wrapped_json_correctness_test] | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ | ✅ |

✅ = passed<br />
❌ = failed<br />
⚠️ = unknown, please click into the test for an explanation


[docs.performance]: ./performance.md
[urls.disk_buffer_persistence_correctness_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/disk_buffer_persistence_correctness
[urls.file_rotate_create_correctness_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_create_correctness
[urls.file_rotate_truncate_correctness_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_truncate_correctness
[urls.file_truncate_correctness_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/file_truncate_correctness
[urls.sighup_correctness_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/sighup_correctness
[urls.test_harness#how-it-works]: https://github.com/timberio/vector-test-harness/#how-it-works
[urls.test_harness]: https://github.com/timberio/vector-test-harness/
[urls.wrapped_json_correctness_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/wrapped_json_correctness
