---
title: Process management
weight: 2
---

This document covers how to manage the Vector process using various interfaces. How you manage the Vector process is largely dependent on how you installed Vector.

## Administrate

{{< administrate >}}

## How it works

### Signals

Signal | Description
:------|:-----------
`SIGTERM` | Initiates a graceful shutdown process
`SIGHUP` | Reloads configuration on the fly

### Exit codes

A full list of exit codes can be found in the [`exitcodes` Rust crate][exitcodes]. The codes that Vector uses:

Code | Description
:----|:-----------
`0` | No error
`1` | Exited with error
`78` | Bad [configuration]

[configuration]: /docs/reference/configuration
[exitcodes]: https://docs.rs/exitcode/latest/exitcode/#constants
