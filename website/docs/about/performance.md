---
title: Performance
---

As part of Vector's development, we invested in a robust [test \
harness][urls.test_harness] that provided a data-driven testing environment.
Below are the results. To learn more, click on each test, or visit the
["How It Works" section][urls.test_harness#how-it-works] in the test harness
README.

## Tests

Higher throughput is better. The winner is in **bold**. Click on the test name for more detail.

| Test | Vector | Filebeat | FluentBit | FluentD | Logstash | SplunkUF | SplunkHF |
| ---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| [TCP to Blackhole][urls.tcp_to_blackhole_performance_test] | _**86MiB/s**_ | n/a | 64.4MiB/s | 27.7MiB/s | 40.6MiB/s | n/a | n/a |
| [File to TCP][urls.file_to_tcp_performance_test] | _**76.7MiB/s**_ | 7.8MiB/s | 35MiB/s | 26.1MiB/s | 3.1MiB/s | 40.1MiB/s | 39MiB/s |
| [Regex Parsing][urls.regex_parsing_performance_test] | 13.2MiB/s | n/a | _**20.5MiB/s**_ | 2.6MiB/s | 4.6MiB/s | n/a | 7.8MiB/s |
| [TCP to HTTP][urls.tcp_to_http_performance_test] | _**26.7MiB/s**_ | n/a | 19.6MiB/s | <1MiB/s | 2.7MiB/s | n/a | n/a |
| [TCP to TCP][urls.tcp_to_tcp_performance_test] | 69.9MiB/s | 5MiB/s | 67.1MiB/s | 3.9MiB/s | 10MiB/s | _**70.4MiB/s**_ | 7.6MiB/s |


[urls.file_to_tcp_performance_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/file_to_tcp_performance
[urls.regex_parsing_performance_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/regex_parsing_performance
[urls.tcp_to_blackhole_performance_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_blackhole_performance
[urls.tcp_to_http_performance_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_http_performance
[urls.tcp_to_tcp_performance_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_tcp_performance
[urls.test_harness#how-it-works]: https://github.com/timberio/vector-test-harness/#how-it-works
[urls.test_harness]: https://github.com/timberio/vector-test-harness/
