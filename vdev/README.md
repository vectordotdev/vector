# vdev

---

The V(ector) Dev(elopment) tool.

This is the command line tooling for Vector development. You don't need to use or install this
unless you are doing development in the Vector repo.

Table of Contents:

- [Pre-requisites](#pre-requisites)
- [Installation](#installation)
- [Running Tests](#running-tests)
  - [Running Integration tests](#running-integration-tests)
- [Developing vdev](#developing-vdev)

## Pre-requisites

This assumes that you have the following tools installed:

- [git](https://git-scm.com/)
- [cargo](https://rustup.rs/)
- [docker](https://www.docker.com/)
- [npm](https://www.npmjs.com/)
- [ruby](https://www.ruby-lang.org/en/)

Some other tools may need to be installed depending on the command you are running. All other
dependencies can be installed by running

```sh
./scripts/environment/prepare.sh
```

## Installation

Note that installation is not required, since `cargo vdev` commands will invoke vdev from within the
repository.

Run the following command from the root of the Vector repository:

```sh
cargo install -f --path vdev
```

You can also install vdev from [crates.io](https://crates.io/crates/vdev)

```sh
cargo install vdev
```

Or using [binstall](https://github.com/cargo-bins/cargo-binstall), which will download a
pre-compiled binary (fastest)

```sh
cargo binstall vdev
```


## Running Tests

Unit tests can be run by calling `make test`.

### Running Integration tests

Integration tests require docker or podman to run.

Integration tests are not run by default when running `make test`. Instead, they are accessible via the integration subcommand `cargo vdev int` (example: `cargo vdev int test aws` runs aws-related integration tests).

You should use `./scripts/run-integration-test.sh`, which is the wrapper used by CI and which suits most development needs. Integration tests require a `cargo vdev int start`, `cargo vdev int test`, and `cargo vdev int stop`, which the script handles automatically. You can find the list of available integration tests using `cargo vdev int show`.


## Developing vdev

The CLI uses [Clap](https://github.com/clap-rs/clap) with the `derive` construction mechanism and is stored in the [commands](src/commands) directory.

Every command group/namespace has its own directory with a `cli` module, including the root `vdev` command group. All commands have an `exec` method that provides the actual implementation, which in the case of command groups will be calling sub-commands.
