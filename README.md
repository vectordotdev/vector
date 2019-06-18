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

#### Setup

* [**Installation**][installation]
* [**Getting started**][getting_started]
* [**Migrating**][migrating]
* [**Deployment**][deployment] - [topologies], [roles]

#### Usage

* [**Configuration**][configuration] - [sources], [transforms], [sinks]
* [**Administration**][administration] - [cli], [start], [stop], [reload], [update]
* [**Guides**][guides]

#### Resources

* [**Community**][community]
* [**Download**][releases]
* [**Roadmap**][roadmap]

---

## Performance

| Test | Vector | Filebeat | FluentBit | FluentD | Logstash | SplunkUF | SplunkHF |
| ---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| [TCP to Blackhole](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_blackhole_performance) | _**`86mib/s`**_ | `n/a` | `64.4mib/s` | `27.7mib/s` | `40.6mib/s` | `n/a` | `n/a` |
| [File to TCP](https://github.com/timberio/vector-test-harness/tree/master/cases/file_to_tcp_performance) | **`76.7mib/s`** | `7.8mib/s` | `35mib/s` | `26.1mib/s` | `3.1mib/s` | `40.1mib/s` | `39mib/s` |
| [Regex Parsing](https://github.com/timberio/vector-test-harness/tree/master/cases/regex_parsing_performance) | `13.2mib/s` | `n/a` | **`20.5mib/s`** | `2.6mib/s` | `4.6mib/s` | `n/a` | `7.8mib/s` |
| [TCP to HTTP](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_http_performance) | **`26.7mib/s`** | `n/a` | `19.6mib/s` | `<1mib/s` | `2.7mib/s` | `n/a` | `n/a` |
| [TCP to TCP](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_tcp_performance) | `69.9mib/s` | `5mib/s` | `67.1mib/s` | `3.9mib/s` | `10mib/s` | **`70.4mib/s`** | `7.6mib/s` |

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

## Development

### Sample Logs

We use `flog` to build a sample set of log files to test sending logs from a file. This can
be done with the following commands on mac with homebrew.

```bash
brew tap mingrammer/flog
brew install flog
$ flog --bytes $((100 * 1024 * 1024)) > sample.log
```

This will create a `100MiB` sample log file in the `sample.log` file.

### Building

Vector compiles with [Rust][rust] 1.34.0 (stable) or newer. In general, Vector tracks the
latest stable release of the Rust compiler.

Building is very easy, all you need to do is install Rust:

```bash
curl https://sh.rustup.rs -sSf | sh
```

And then use `cargo` to build:

```bash
cargo build
```

### Testing

Testing is a bit more complicated, this because to test all the sinks we need to stand
up local mock versions of the sources we send logs too. To do this we use `docker` and 
`docker-compose` to stand up this environment. To run the full test suit you can run

```bash
# Test everything that does not require docker
cargo test -- --test-threads=4

# Test everything that can also be tested with docker
cargo test --features docker
```

### Benchmarking

You can run the internal project benchmarks with

```
cargo bench
```

### Test Harness

In addition, we maintain a separate higher-level [test harness][test_harness] designed
for performance and correctness testing.


### Code Style

We use `rustfmt` on `stable` to format our code and CI will verify that your code follows
this format style. To run the following command make sure `rustfmt` has been installed on
the stable toolchain locally.

```bash
cargo fmt
```

Once that's all passing and you're happy with your change, go ahead and commit.
For small, unobtrusive changes, committing to directly to master is fine. For
anything that merits more discussion or visibility, committing to a branch and
opening a pull request is preferred. Just use your best judgement and if you're
unsure, open a pull request.

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
[getting_started]: https://docs.vectorproject.io/setup/getting_started
[governance]: https://docs.vectorproject.io/about/use_cases/governance
[guides]: https://docs.vectorproject.io/usage/guides
[installation]: https://docs.vectorproject.io/setup/installation
[lock-in]: https://docs.vectorproject.io/about/use_cases/lock-in
[migrating]: https://docs.vectorproject.io/setup/migrating
[multi-cloud]: https://docs.vectorproject.io/about/use_cases/multi-cloud
[performance]: https://docs.vectorproject.io/performance
[releases]: https://github.com/timberio/vector/releases
[reload]: https://docs.vectorproject.io/usage/administration/reloading
[roadmap]: https://github.com/timberio/vector/milestones?direction=asc&sort=title&state=open
[roles]: https://docs.vectorproject.io/setup/deployment/roles
[rust]: https://www.rust-lang.org/
[security]: https://docs.vectorproject.io/about/use_cases/security-and-compliance
[service]: https://docs.vectorproject.io/setup/deployment/roles/service
[sinks]: https://docs.vectorproject.io/usage/configuration/sinks
[sources]: https://docs.vectorproject.io/usage/configuration/sources
[start]: https://docs.vectorproject.io/usage/administration/starting
[stop]: https://docs.vectorproject.io/usage/administration/stopping
[test_harness]: https://github.com/timberio/vector-test-harness
[topologies]: https://docs.vectorproject.io/setup/deployment/topologies
[transforms]: https://docs.vectorproject.io/usage/configuration/transforms
[update]: https://docs.vectorproject.io/usage/administration/updating
[use_cases]: https://docs.vectorproject.io/use_cases
