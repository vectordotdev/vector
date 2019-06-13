---
description: Vector command line interface
---

# CLI

This document will cover the `vector` binary command line interface.

## Quick Start

At any time, you can display a full list of available commands and options via the `vector --help` command:

```text
$ vector --help
Vector 1.0
timber.io

USAGE:
    vector [FLAGS] [OPTIONS] --config <FILE>

FLAGS:
    -h, --help               Prints help information
    -r, --require-healthy    Causes vector to immediate exit on startup if any sinks having failing healthchecks
    -V, --version            Prints version information

OPTIONS:
    -c, --config <FILE>                  Sets a custom config file
    -m, --metrics-addr <metrics-addr>    The address that metrics will be served from [default: 127.0.0.1:8888]
```

Each command is covered in more detail within it's respective administration document, which you can find in the left-hand navigation.

