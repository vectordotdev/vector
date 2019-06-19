<p align="center">
  <strong><a href="https://vectorproject.typeform.com/to/wV0DC8">Join our beta!<a/> Timber is looking for select beta testers to help shape the future of Vector.</strong>
</p>

<p align="center">
  <img src="./docs/.gitbook/assets/readme_diagram.svg" alt="Vector">
</p>

<p align="center">
  <a href="https://github.com/timberio/vector/releases"><img src="https://img.shields.io/github/release/timberio/vector.svg"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/timberio/vector.svg"></a>
  <a href="https://circleci.com/gh/timberio/vector"><img src="https://circleci.com/gh/timberio/vector/tree/master.svg?style=shield"></a>
  <a href="https://chat.vectorproject.io/badge.svg"><img src="https://chat.vectorproject.io/badge.svg"></a>
</p>

Vector is a [high-performance][performance] router for observability data. It makes
[collecting][sources], [transforming][transforms], and [sending][sinks] logs, metrics, and events
easy. It decouples data collection & routing from your services, [future proofing][lock-in] your
pipeline, and enabling you to freely adopt best-in-class services over time, among
[many other benefits][use_cases].

Built in [Rust][rust], Vector places high-value on [performance], [correctness], and
[operator friendliness][administration]. It compiles to a single static binary and is designed
to be [deployed][deployment] across your entire infrastructure, serving both as a
light-weight [agent] and a highly efficient [service], making it the single tool you need to
get data from A to B.

---

#### About

* [**Use cases**][use_cases]
* [**Performance**][performance]
* [**Correctness**][correctness]
* [**Concepts**][concepts]
* [**Data model**][data_model]
* [**Guarantees**][guarantees]

#### Setup

* [**Installation**][installation]
* [**Getting started**][getting_started]
* [**Migrating**][migrating]
* [**Deployment**][deployment] - [topologies], [roles]

#### Usage

* [**Configuration**][configuration] - [sources], [transforms], [sinks]
* [**Administration**][administration] - [cli], [starting], [stopping], [reloading], [updating]
* [**Guides**][guides]

#### Resources

* [**Community**][community] - [forum], [slack chat][slack]
* [**Download**][releases]
* [**Roadmap**][roadmap]

#### Meta

* [**Changelog**](CHANGELOG.md)
* [**Contributing**](CONTRIBUTING.md)
* [**Releasing**](RELEASING.md)

---

## Features

* **Fast** - Built in [Rust][rust], Vector is [fast and memory efficient][performance]. It has no runtime or garbage collector, and is designed to handle the most demanding environments.
* **Correct** - Vector is obsessed with [getting the details right][correctness].
* **Vendor Neutral** - Vector does not favor any specific storage. It fosters a fair, open ecosystem with the user's best interest in mind.
* **Agent Or Service** - Vector aims to be the single, and only, tool needed to get data from A to B, deploying as an [agent] or [service].
* **Logs, Metrics, or Events** - Vector unifies logs, metrics, and events at the source, making it collect and ship all observability data.
* **Clear Guarantees** - Vector is [clear on it's guarantees][guarantees], helping you to make the appropriate trade offs for your use case.
* **Easy To Deploy** - Vector compiles to a single static binary with no runtime, making it easy to deploy and manage.
* **Hot Reload** - Designed to [reload configuration][reloading], on the fly, without restarting or skipping a beat.

## Performance

| Test | Vector | Filebeat | FluentBit | FluentD | Logstash | SplunkUF | SplunkHF |
| ---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| [TCP to Blackhole](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_blackhole_performance) | _**86mib/s**_ | n/a | 64.4mib/s | 27.7mib/s | 40.6mib/s | n/a | n/a |
| [File to TCP](https://github.com/timberio/vector-test-harness/tree/master/cases/file_to_tcp_performance) | _**76.7mib/s**_ | 7.8mib/s | 35mib/s | 26.1mib/s | 3.1mib/s | 40.1mib/s | 39mib/s |
| [Regex Parsing](https://github.com/timberio/vector-test-harness/tree/master/cases/regex_parsing_performance) | 13.2mib/s | n/a | _**20.5mib/s**_ | 2.6mib/s | 4.6mib/s | n/a | 7.8mib/s |
| [TCP to HTTP](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_http_performance) | _**26.7mib/s**_ | n/a | 19.6mib/s | <1mib/s | 2.7mib/s | n/a | n/a |
| [TCP to TCP](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_tcp_performance) | 69.9mib/s | 5mib/s | 67.1mib/s | 3.9mib/s | 10mib/s | _**70.4mib/s**_ | 7.6mib/s |

To learn more about our performance tests, please see the [Vector test harness][test_harness].

## Correctness

| Test | Vector | Filebeat | FluentBit | FluentD | Logstash | Splunk UF | Splunk HF |
| ---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| [Disk Buffer Persistence](https://github.com/timberio/vector-test-harness/tree/master/cases/disk_buffer_performance) | ✅ | ✅ | ❌ | ❌ | ⚠️\* | ✅ | ✅ |
| [File Rotate \(create\)](https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_create_correctness) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| [File Rotate \(copytruncate\)](https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_truncate_correctness) | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| [File Truncation](https://github.com/timberio/vector-test-harness/tree/master/cases/file_truncate_correctness) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| [Process \(SIGHUP\)](https://github.com/timberio/vector-test-harness/tree/master/cases/sighup_correctness) | ✅ | ❌ | ❌ | ❌ | ⚠️\* | ✅ | ✅ |
| TCP Streaming | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| [JSON \(wrapped\)](https://github.com/timberio/vector-test-harness/tree/master/cases/wrapped_json_correctness) | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ | ✅ |

To learn more about our performance tests, please see the [Vector test harness][test_harness].

## License

Copyright 2019, Vector Authors. All rights reserved.

Licensed under the Apache License, Version 2.0 (the "License"); you may not
use these files except in compliance with the License. You may obtain a copy
of the License at

http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
License for the specific language governing permissions and limitations under
the License.

---

<p align="center">
  Developed with ❤️ by <strong><a href="https://timber.io">Timber.io</a></strong>
</p>

[administration]: https://docs.vectorproject.io/usage/administration
[agent]: https://docs.vectorproject.io/setup/deployment/roles/agent
[backups]: https://docs.vectorproject.io/about/use_cases/backups
[cli]: https://docs.vectorproject.io/administration/cli
[community]: https://vectorproject.io/community/
[configuration]: https://docs.vectorproject.io/usage/configuration
[concepts]: https://docs.vectorproject.io/about/concepts
[cost]: https://docs.vectorproject.io/about/use_cases/cost
[correctness]: https://docs.vectorproject.io/comparisons
[data_model]: https://docs.vectorproject.io/about/data_model
[deployment]: https://docs.vectorproject.io/setup/deployment
[forum]: https://forum.vectorproject.io
[getting_started]: https://docs.vectorproject.io/setup/getting_started
[governance]: https://docs.vectorproject.io/about/use_cases/governance
[guarantees]: https://docs.vectorproject.io/aboout/guarantees
[guides]: https://docs.vectorproject.io/usage/guides
[installation]: https://docs.vectorproject.io/setup/installation
[lock-in]: https://docs.vectorproject.io/about/use_cases/lock-in
[migrating]: https://docs.vectorproject.io/setup/migrating
[multi-cloud]: https://docs.vectorproject.io/about/use_cases/multi-cloud
[performance]: https://docs.vectorproject.io/performance
[releases]: https://github.com/timberio/vector/releases
[reloading]: https://docs.vectorproject.io/usage/administration/reloading
[roadmap]: https://github.com/timberio/vector/milestones?direction=asc&sort=title&state=open
[roles]: https://docs.vectorproject.io/setup/deployment/roles
[rust]: https://www.rust-lang.org/
[security]: https://docs.vectorproject.io/about/use_cases/security-and-compliance
[service]: https://docs.vectorproject.io/setup/deployment/roles/service
[sinks]: https://docs.vectorproject.io/usage/configuration/sinks
[slack]: https://chat.vectorproject.io
[sources]: https://docs.vectorproject.io/usage/configuration/sources
[starting]: https://docs.vectorproject.io/usage/administration/starting
[stopping]: https://docs.vectorproject.io/usage/administration/stopping
[test_harness]: https://github.com/timberio/vector-test-harness
[test_harness_how_it_works]: https://github.com/timberio/vector-test-harness#how-it-works
[topologies]: https://docs.vectorproject.io/setup/deployment/topologies
[transforms]: https://docs.vectorproject.io/usage/configuration/transforms
[updating]: https://docs.vectorproject.io/usage/administration/updating
[use_cases]: https://docs.vectorproject.io/use_cases
