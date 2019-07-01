# Performance

As part of Vector's development, we invested in a robust [test \
harness][url.test_harness] that provided a data-driven testing environment.
Below are the results. To learn more, click on each test, or visit the
["How It Works" section][url.test_harness.how-it-works] in the test harness
README.

## Tests

Higher throughput is better. The winner is in **bold**. Click on the test name for more detail.

| Test | Vector | Filebeat | FluentBit | FluentD | Logstash | SplunkUF | SplunkHF |
| ---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| [TCP to Blackhole][url.tcp_to_blackhole_performance_test] | _**86mib/s**_ | n/a | 64.4mib/s | 27.7mib/s | 40.6mib/s | n/a | n/a |
| [File to TCP][url.file_to_tcp_performance_test] | _**76.7mib/s**_ | 7.8mib/s | 35mib/s | 26.1mib/s | 3.1mib/s | 40.1mib/s | 39mib/s |
| [Regex Parsing][url.regex_parsing_performance_test] | 13.2mib/s | n/a | _**20.5mib/s**_ | 2.6mib/s | 4.6mib/s | n/a | 7.8mib/s |
| [TCP to HTTP][url.tcp_to_http_performance_test] | _**26.7mib/s**_ | n/a | 19.6mib/s | <1mib/s | 2.7mib/s | n/a | n/a |
| [TCP to TCP][url.tcp_to_tcp_performance_test] | 69.9mib/s | 5mib/s | 67.1mib/s | 3.9mib/s | 10mib/s | _**70.4mib/s**_ | 7.6mib/s |


[url.file_to_tcp_performance_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/file_to_tcp_performance
[url.regex_parsing_performance_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/regex_parsing_performance
[url.tcp_to_blackhole_performance_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_blackhole_performance
[url.tcp_to_http_performance_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_http_performance
[url.tcp_to_tcp_performance_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_tcp_performance
[url.test_harness.how-it-works]: https://github.com/timberio/vector-test-harness/#how-it-works
[url.test_harness]: https://github.com/timberio/vector-test-harness/
