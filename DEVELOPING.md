# Development

This document covers the basics of developing in Vector. In this document:

<!-- MarkdownTOC autolink="true" style="ordered" -->

1. [Prerequisites](#prerequisites)
1. [Setup](#setup)
1. [Directory Structure](#directory-structure)
1. [Makefile](#makefile)
1. [Testing](#testing)
  1. [Docker](#docker)
  1. [Sample Logs](#sample-logs)
1. [Benchmarking](#benchmarking)
1. [Building](#building)
1. [Testing](#testing-1)
1. [Benchmarking](#benchmarking-1)
1. [CI](#ci)
1. [Code Style](#code-style)

<!-- /MarkdownTOC -->

## Prerequisites

1. **You are familiar with the [docs](https://docs.vector.dev).**
2. **You have read the [Contributing](/CONTRIBUTING.md) guide.**
3. **You know about the [Vector community](https://vector.dev/community/),
   use this help.**

## Setup

1. Install Rust:

   ```bash
   curl https://sh.rustup.rs -sSf | sh
   ```

2. [Install Docker](https://docs.docker.com/docker-for-mac/install/). Docker
   containers are used for mocking Vector's integrations.

## Directory Structure

* [`/benches`](/benches) - Internal benchmarks.
* [`/config`](/config) - Public facing Vector config, included in releases.
* [`/distribution`](/distribution) - Distribution artifacts for various targets.
* [`/docs`](/docs) - https://docs.vector.dev source.
* [`/lib`](/lib) - External libraries.
* [`/proto`](/proto) - Protobuf definitions.
* [`/scripts`](/scripts) - Scripts used to generate docs and maintain the repo.
* [`/tests`](/tests) - Various high-level test cases.

## Makefile

Vector includes a [`Makefile`](/Makefile) that exposes top-level commands. Ex:

- `make test`
- `make build`
- `make generate_docs`

The various commands are below within their respective sections.

## Testing

```bash
make test
```

### Docker

We use docker to mock out external services for our tests. The `make test`
command calls `docker-compose up -d` which is defined by the
`docker-compose.yml` file.

### Sample Logs

We use `flog` to build a sample set of log files to test sending logs from a file. This can
be done with the following commands on mac with homebrew.

```bash
brew tap mingrammer/flog
brew install flog
$ flog --bytes $((100 * 1024 * 1024)) > sample.log
```

This will create a `100MiB` sample log file in the `sample.log` file.

## Benchmarking

```bash
make bench
```

## Building

```bash
make build
```

## Testing

Testing is a bit more complicated, this because to test all the sinks we need to stand
up local mock versions of the sources we send logs too. To do this we use `docker` and 
`docker-compose` to stand up this environment. To run the full test suit you can run

```bash
# Test everything that does not require docker
cargo test -- --test-threads=4

# Test everything that can also be tested with docker
cargo test --features docker
```

## Benchmarking

You can run the internal project benchmarks with

```
cargo bench
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
cargo fmt
```
