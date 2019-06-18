# Performance

This section demonstrates performance across common scenarios. As part of Vector's development, we invested in a robust [test harness](https://github.com/timberio/vector-test-harness) that provided a data-driven testing environment. Below are the results. To learn more, see [How It Works](performance.md#how-it-works).

_Please note_, these are not audited independent benchmarks, they are our own internal performance tests to ensure Vector met our standards. Use these results as a point of reference. [Pull requests](https://github.com/timberio/vector-test-harness/pulls) are welcome to improve the tests.

## Tests

Higher throughput is better. The winner is in **bold**. Click on the test name for more detail.

| Test | Vector | Filebeat | FluentBit | FluentD | Logstash | SplunkUF | SplunkHF |
| ---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| [TCP to Blackhole](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_blackhole_performance) | _**`86mib/s`**_ | `n/a` | `64.4mib/s` | `27.7mib/s` | `40.6mib/s` | `n/a` | `n/a` |
| [File to TCP](https://github.com/timberio/vector-test-harness/tree/master/cases/file_to_tcp_performance) | **`76.7mib/s`** | `7.8mib/s` | `35mib/s` | `26.1mib/s` | `3.1mib/s` | `40.1mib/s` | `39mib/s` |
| [Regex Parsing](https://github.com/timberio/vector-test-harness/tree/master/cases/regex_parsing_performance) | `13.2mib/s` | `n/a` | **`20.5mib/s`** | `2.6mib/s` | `4.6mib/s` | `n/a` | `7.8mib/s` |
| [TCP to HTTP](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_http_performance) | **`26.7mib/s`** | `n/a` | `19.6mib/s` | `<1mib/s` | `2.7mib/s` | `n/a` | `n/a` |
| [TCP to TCP](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_tcp_performance) | `69.9mib/s` | `5mib/s` | `67.1mib/s` | `3.9mib/s` | `10mib/s` | **`70.4mib/s`** | `7.6mib/s` |

## How It Works

Vector developed a robust test harness to collect this performance data. You can click on each test above to learn more, or view the [test harness readme](https://github.com/timberio/vector-test-harness), which provides a deep dive into the design.

