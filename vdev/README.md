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
- [Coverage publishing](#coverage-publishing)
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


## Coverage publishing

The Coverage workflow runs the workspace unit/component/CLI/API suites and all integration
and E2E tests on one resolved commit. Unit reports include workspace libraries. Integration
coverage measures the test process; E2E runner coverage is excluded because it does not
instrument the separately launched Vector service.

Coverage uses three `cargo vdev build coverage` operations:

- `plan` records the tested commit, workflow run, service matrix, and expected environment reports.
- `seal` normalizes source paths and records each producer's report checksums and provenance.
- `publish` validates the entire collection before uploading original reports with `unit` and
  `integration` flags. `--validate-only` writes the same line-union summary without uploading.

CI builds the workflow revision's vdev once and shares it with coverage jobs, so these commands
also work when testing an older review commit. Both Datadog commit overrides use the resolved
tested SHA, even if the pull request advances while CI runs. Missing, unexpected, corrupt, or
mismatched reports fail publication. A unit report with no library coverage is also rejected.

Locally, `cargo vdev integration run --coverage <service>` retains individual
`target/coverage/lcov-<environment>.info` files. A new run removes previous coverage reports in
that directory to prevent stale results from satisfying a retry.

The final job archives the originals, manifests, plan, and validation summary for 90 days.
Datadog aggregates the uploaded reports. Ingestion is incremental: prevalidation prevents a
known incomplete collection from being submitted, but a network failure during upload can
still leave a partial result. A successful publisher job is the completion signal; the
Datadog percentage alone is not. Re-running only the publisher reuses the validated artifacts.

## Developing vdev

If you are actively developing vdev itself, install from your working tree so the binary on `PATH` reflects your local changes:

```sh
cargo install -f --path vdev
```

Package repository commands are disabled in ordinary vdev builds. To enable them locally:

```sh
cargo run --package vdev --features package-repositories -- release repositories --help
```

The separate `vdev-package-repositories-x86_64-unknown-linux-gnu` release archive enables
this feature; ordinary vdev binaries omit it. Repository commands run natively on Linux
and need `apt-utils`, `dpkg-dev`, `createrepo-c`, `rpm`, and `gnupg`. Key files use private,
automatically cleaned directories in `/dev/shm`. Until the production integration is
installed, pass `--config` with the repository configuration and its public key.

## Releasing vdev

To release vdev, open a PR that bumps the `version` field in `vdev/Cargo.toml`.
After it merges to `master`, CI automatically tags the version and publishes the binaries and crate.
