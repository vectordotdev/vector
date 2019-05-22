<p align="center">
  <strong>Join our beta! Timber is looking for select beta testers to hep shape the future of Vector</strong>
</p>

<p align="center">
  <img src="./assets/readme_diagram.svg" alt="Vector">
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/timberio/vector.svg"></a>
  <a href="https://circleci.com/gh/timberio/vector"><img src="https://circleci.com/gh/timberio/vector/tree/master.svg?style=shield"></a>
  <a href="https://slack.linkerd.io/badge.svg"><img src="https://slack.linkerd.io/badge.svg"></a>
</p>

Vector is a high-performance routing layer for observability data. It makes [collecting][sources],
[transforming][transforms], and [routing][sinks] logs and metrics easy. It's designed to be
[deployed][deployment] across your entire infrastructure, serving both as a light-weight [agent]
and a highly efficient [service].

Built in [Rust][rust], Vector has a [*keen* focus on performance][performance] and
[operator friendliness][administration]. Its purpose is to decouple routing from the services,
giving you complete flexibility to adopt the services of your choice, among
[many other benefits][use_cases].

---

#### About

* [**Use cases**][use_cases] - [cost], [governance], [security], [backups], [lock-in], [multi-cloud]
* [**Performance**][performance]
* [**Correctness**][correctness]
* [**Concepts**][concepts]
* [**Data Model**][data_model]

#### Setup

* [**Installation**][installation]
* [**Getting started**][getting_started]
* [**Deployment**][deployment] - [topologies], [roles]

#### Usage

* [**Configuration**][configuration] - [sources], [transforms], [sinks]
* [**Administration**][administration] - [start], [stop], [reload], [update]
* [**Guides**][guides]

---

## Why?

With the shift towards cloud-native archictectures the approch to managing observability data
is rapidly changing. Legacy approaches to managing logare becoming increasingly awkward and
untenable. From sheer scaling and management to cost, sophisticated architectures no longer
shove log data into a single.

- Cloud-scale
- Sophi
- Mercy of vendors
- Siophisticated pipelines are composed of multiple systems,
- Achieving these sophisticted setups requires a talented team, deep big dat experience, and
deep pockets. Vectors aims to tear that down, enabling engineers to build out sophisticated
pipelines by simply specifying their toploogy in a configuration file.

## Performance

## Correctness

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
cargo test

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

---

<p align="center">
  Developed with ❤️ by <strong><a href="https://timber.io">Timber.io</a></strong>
</p>

[administration]: https://docs.vectorproject.io/usage/administration
[agent]: https://docs.vectorproject.io/setup/deployment/roles/agent
[backups]: https://docs.vectorproject.io/about/use_cases/backups
[configuration]: https://docs.vectorproject.io/usage/configuration
[concepts]: https://docs.vectorproject.io/about/concepts
[cost]: https://docs.vectorproject.io/about/use_cases/cost
[correctness]: https://docs.vectorproject.io/about/correctness
[data_model]: https://docs.vectorproject.io/about/data_model
[deployment]: https://docs.vectorproject.io/setup/deployment
[getting_started]: https://docs.vectorproject.io/setup/getting_started
[governance]: https://docs.vectorproject.io/about/use_cases/governance
[guides]: https://docs.vectorproject.io/usage/guides
[installation]: https://docs.vectorproject.io/setup/installation
[lock-in]: https://docs.vectorproject.io/about/use_cases/lock-in
[multi-cloud]: https://docs.vectorproject.io/about/use_cases/multi-cloud
[performance]: https://docs.vectorproject.io/about/performance
[reload]: https://docs.vectorproject.io/usage/administration/reloading
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
