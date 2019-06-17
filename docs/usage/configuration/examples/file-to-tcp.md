---
description: >-
  Example configuration that tails a file and forwards it's data over a TCP
  socket
---

# File to TCP

```c
[sources.log]
    type = "file"
    location = "/var/log/log.log"

[sinks.tcp_out]
    inputs = ["log"]
    address = "0.0.0.0:9000"
```

## Description

The above example tails the `/var/log/log.log` file and writes each individual line to a TCP socket. This is an incredibly simple example that does not concern itself with structuring or parsing. As such, the TCP socket simply receives new line delimited text lines.

## Output

Vector takes the path of least surprise for

