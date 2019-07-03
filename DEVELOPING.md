# Development

This document covers the basics of developing in Vector. In this document:

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Prerequisites](#prerequisites)
1. [Setup](#setup)
1. [Directory Structure](#directory-structure)
1. [Makefile](#makefile)
1. [Testing](#testing)
   1. [Sample Logs](#sample-logs)
1. [Testing](#testing-1)
1. [Building](#building)
1. [Checking](#checking)
1. [Running](#running)
1. [Benchmarking](#benchmarking)
1. [CI](#ci)
1. [Code Style](#code-style)

<!-- /MarkdownTOC -->

## Prerequisites

1. **You are familiar with the [docs](https://docs.vector.dev).**
2. **You have read the [Contributing](/CONTRIBUTING.md) guide.**
3. **You know about the [Vector community](https://vector.dev/community/),
   use this help.**

## Setup

1. Install Rust via [`rustup`](https://rustup.rs/):

   ```bash
   curl https://sh.rustup.rs -sSf | sh
   ```

2. [Install Docker](https://docs.docker.com/install/). Docker
   containers are used for mocking Vector's integrations.

## Directory Structure

* [`/benches`](/benches) - Internal benchmarks.
* [`/config`](/config) - Public facing Vector config, included in releases.
* [`/distribution`](/distribution) - Distribution artifacts for various targets.
* [`/docs`](/docs) - https://docs.vector.dev source.
* [`/lib`](/lib) - External libraries that do not depend on `vector` but are used within the project.
* [`/proto`](/proto) - Protobuf definitions.
* [`/scripts`](/scripts) - Scripts used to generate docs and maintain the repo.
* [`/tests`](/tests) - Various high-level test cases.

## Makefile

Vector includes a [`Makefile`](/Makefile) that exposes top-level commands. Ex:

- `make test`
- `make build`
- `make check`
- `make run`
- `make bench`
- `make generate_docs`

The various commands are below within their respective sections.

## Testing

This command will attempt to run `docker-compose up -d` then follow up with `cargo test --features docker` command. Currently, it limits the test threads to 4 as we spin up many `tokio` runtimes which sometimes causes fd limit exceeded errors.
 
```bash
make test
```

Testing is a bit more complicated, this is because to test all the sinks we need to stand
up local mock versions of the sources we send logs too. To do this we use `docker` and 
`docker-compose` to stand up this environment. To run the full test suit you can run

```bash
# To run every test
make test

# To test things that do not require docker
make test-simple
```

### Sample Logs

We use `flog` to build a sample set of log files to test sending logs from a file. This can
be done with the following commands on mac with homebrew. Installation instruction for flog can be found [here](https://github.com/mingrammer/flog#installation).

```bash
flog --bytes $((100 * 1024 * 1024)) > sample.log
```

This will create a `100MiB` sample log file in the `sample.log` file.

## Building

This will _compile_ the `vector` project in _debug_ mode. Be aware that this mode is not optimized and may run slowly. Generally speaking, `make build` can be quite slow and poor for quick feedback. In most cases while developing `vector` you may want to use `make check` instead.

```bash
make build
```

## Checking

This command will internally call `cargo check` which runs all the checks the compiler would run normally without actually doing any linking or codegen. This is a lot quicker than running `make build` and is perfect for when you want to get quick feedback. To ensure that you check every code path this will check every feature, every target (including tests and benches), and test every crate in the workspace. This command is what is run on the `check-stable` CI job.

```bash
make check
```

## Running

Vector can also be run in debug mode via calling `make run`. Though this may not be sufficient since you may need to pass arguments to the `vector` binary. To do this you can do the following:

```bash
# To run it simply
make run

# To run it with a custom config
cargo run -- -c <path to config>
```

## Benchmarking

This will run our internal set of benchmarks mainly used for find regressions and comparing implementations. All the benchmarks live within `/benches`.

```bash
make bench
```

## CI

Currently Vector uses [CircleCI](https://circleci.com). The build process
is defined in `/.circleci/config.yml`. This delegates heavily to the
[`distribution/docker`](/distribution/docker) folder where Docker images are
defined for all of our testing, building, verifying, and releasing.

## Code Style

We use `rustfmt` on `stable` to format our code and CI will verify that your code follows
this format style. To run the following command make sure `rustfmt` has been installed on
the stable toolchain locally.

```bash
# To install rustfmt
rustup component add rustfmt

# To format the code
make fmt
```
