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

Some other tools may need to be installed depending on the command you are running. All other
dependencies can be installed by running

```sh
./scripts/environment/prepare.sh
```

## Installation

CI installs vdev from a published binary release via [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) and never builds it from source. To match that locally:

```sh
./scripts/environment/prepare.sh --modules=vdev
```

This pins the vdev version defined in `prepare.sh` and fetches the matching pre-compiled binary.

For a quick install of the latest published vdev (not pinned):

```sh
cargo binstall vdev
```

If binstall is unavailable, fall back to compiling from crates.io:

```sh
cargo install vdev
```

Installation is otherwise optional: from within the Vector repository, `cargo vdev` works via a cargo alias that compiles vdev from source on each invocation. That path is fine for occasional use but is slower than a binstalled binary.


## Running Tests

Unit tests can be run by calling `make test`.

### Running Integration tests

Integration tests require docker or podman to run.

Integration tests are not run by default when running `make test`. Instead, they are accessible via the integration subcommand `cargo vdev int` (example: `cargo vdev int test aws` runs aws-related integration tests).

You should use `./scripts/run-integration-test.sh`, which is the wrapper used by CI and which suits most development needs. Integration tests require a `cargo vdev int start`, `cargo vdev int test`, and `cargo vdev int stop`, which the script handles automatically. You can find the list of available integration tests using `cargo vdev int show`.


## Testing vdev changes in CI

CI consumes a released vdev binary, so changes to `vdev/**` in a PR are not picked up by CI until a new version is published. When in-PR validation of a vdev change is needed, use a release candidate:

1. **Bump the version** in `vdev/Cargo.toml` to the next pre-release (e.g. `0.3.4-rc.1` or `0.3.4-pr.1234`).

2. **Push a tag** from the PR branch:

   ```sh
   git tag vdev-v0.3.4-pr.1234
   git push origin vdev-v0.3.4-pr.1234
   ```

   This triggers `vdev_publish.yml`, which builds and uploads the binaries to a GitHub pre-release. No crates.io publish happens for pre-release tags (`-rc.*` or `-pr.*`).

3. **Bump `VDEV_VERSION`** in `scripts/environment/prepare.sh` to the pre-release version:

   ```sh
   VDEV_VERSION="0.3.4-pr.1234"
   ```

   Commit this to the PR so CI binstalls the pre-release binary.

4. **Validate** by watching the PR's CI runs. Once the change is confirmed, land the PR.

5. **Promote to a stable release**: bump `vdev/Cargo.toml` to `0.3.4`, push `vdev-v0.3.4`, then update `VDEV_VERSION` in `prepare.sh` to `0.3.4`.

The RC tag reuses the existing publish machinery (`vdev_publish.yml`) with no extra steps. `cargo binstall` resolves the version from the GitHub release assets, so crates.io is not involved for RC installs.

## Developing vdev

If you are actively developing vdev itself, install from your working tree so the binary on `PATH` reflects your local changes:

```sh
cargo install -f --path vdev
```

The CLI uses [Clap](https://github.com/clap-rs/clap) with the `derive` construction mechanism and is stored in the [commands](src/commands) directory.

Every command group/namespace has its own directory with a `cli` module, including the root `vdev` command group. All commands have an `exec` method that provides the actual implementation, which in the case of command groups will be calling sub-commands.
