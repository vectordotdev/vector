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
- [Releasing vdev](#releasing-vdev)

## Pre-requisites

This assumes that you have the following tools installed:

- [git](https://git-scm.com/)
- [cargo](https://rustup.rs/)
- [docker](https://www.docker.com/)
- [npm](https://www.npmjs.com/)

Some other tools may need to be installed depending on the command you are running. All other
dependencies can be installed by running

```sh
./scripts/environment/prepare.sh
```

## Installation

To install the same vdev version as CI:

```sh
./scripts/environment/prepare.sh --modules=vdev
```

The version is pinned in `vdev/Cargo.toml`. If its binaries have not been published yet,
`prepare.sh` builds vdev from your working tree.

For a quick install of the latest published vdev (not pinned):

```sh
cargo binstall vdev
```

If binstall is unavailable, fall back to compiling from crates.io:

```sh
cargo install vdev
```

You can also use `cargo vdev` from the Vector repository without installing vdev.


## Running Tests

Unit tests can be run by calling `make test`.

### Running Integration tests

Integration tests require docker or podman to run.

Integration tests are not run by default when running `make test`. Instead, they are accessible via the integration subcommand `cargo vdev int` (example: `cargo vdev int test aws` runs aws-related integration tests).

For most development needs, use `cargo vdev integration run <test-name>`, which handles the full lifecycle (start, test, stop) automatically. You can find the list of available integration tests using `cargo vdev int show`.


## Developing vdev

If you are actively developing vdev itself, install from your working tree so the binary on `PATH` reflects your local changes:

```sh
cargo install -f --path vdev
```

## Releasing vdev

To release vdev, open a PR that bumps the `version` field in `vdev/Cargo.toml`.
After it merges to `master`, CI automatically tags the version and publishes the binaries and crate.
