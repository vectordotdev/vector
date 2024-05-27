# vdev

-----

This is the command line tooling for Vector development.

Table of Contents:

- [Installation](#installation)
- [Configuration](#configuration)
  - [Repository](#repository)
  - [Starship](#starship)
- [CLI](#cli)
- [Running Tests](#running-tests)

## Pre-requisites

This assumes that you have the following tools installed:

- cargo-nextest - https://nexte.st/

## Installation

Run the following command from the root of the Vector repository:

```text
cargo install -f --path vdev
```

## Configuration

### Repository

Setting the path to the repository explicitly allows the application to be used at any time no matter the current working directory.

```text
vdev config set repo .
```

To test, enter your home directory and then run:

```text
vdev exec ls
```

### Starship

A custom command for the [Starship](https://starship.rs) prompt is available.

```toml
format = """
...
${custom.vdev}\
...
$line_break\
...
$character"""

# <clipped>

[custom.vdev]
command = "vdev meta starship"
when = true
# Windows
# shell = ["cmd", "/C"]
# Other
# shell = ["sh", "--norc"]
```

## CLI

The CLI uses [Clap](https://github.com/clap-rs/clap) with the `derive` construction mechanism and is stored in the [commands](src/commands) directory.

Every command group/namespace has its own directory with a `cli` module, including the root `vdev` command group. All commands have an `exec` method that provides the actual implementation, which in the case of command groups will be calling sub-commands.


## Running Tests

Unit tests can be run by calling `cargo vdev test`.

Integration tests are not run by default when running`cargo vdev test`. Instead, they are accessible via the integration subcommand (example: `cargo vdev int test aws` runs aws-related integration tests). You can find the list of available integration tests using `cargo vdev int show`. Integration tests require docker or podman to run.
