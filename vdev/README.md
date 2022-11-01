# vdev

-----

This is the command line tooling for Vector development.

Table of Contents:

- [Installation](#installation)
- [Configuration](#configuration)
  - [Repository](#repository)
  - [Starship](#starship)

## Installation
Run the following command from the root of the vector repository:

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
